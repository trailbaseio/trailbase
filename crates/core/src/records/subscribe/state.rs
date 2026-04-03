use async_channel::{TrySendError, WeakReceiver};
use futures_util::Stream;
use log::*;
use parking_lot::RwLock;
use pin_project_lite::pin_project;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, LazyLock};
use std::task::{Context, Poll};
use trailbase_qs::ValueOrComposite;
use trailbase_schema::QualifiedName;
use trailbase_schema::json::value_to_flat_json;

use crate::auth::User;
use crate::records::RecordApi;
use crate::records::RecordError;
use crate::records::filter::{Filter, qs_filter_to_record_filter};
use crate::records::subscribe::event::{EventPayload, JsonEventPayload};
use crate::records::subscribe::hook::{
  PreupdateHookEvent, RecordAction, install_hook, uninstall_hook,
};
use crate::schema_metadata::ConnectionMetadata;

/// Composite id uniquely identifying a subscription.
///
/// If row_id is Some, this is considered to reference a subscription to a specific record.
#[derive(Clone, Default, Debug)]
pub struct SubscriptionId {
  pub table_name: QualifiedName,
  pub sub_id: i64,
  pub row_id: Option<i64>,
}

/// RAII type for automatically cleaning up subscriptions when the receiving side gets dropped,
/// e.g. client disconnects.
struct AutoCleanupEventStreamState {
  receiver: WeakReceiver<EventCandidate>,
  state: Arc<PerConnectionState>,
  id: SubscriptionId,
}

impl Drop for AutoCleanupEventStreamState {
  fn drop(&mut self) {
    // Subscriptions can be cleaned up either by the sender, i.e. when trying to broker events and
    // tables or records get deleted, or by the client-receiver, e.g. by disconnecting. In the
    // latter case, we need to clean up the subscription.
    if self.receiver.upgrade().is_some() {
      let id = std::mem::take(&mut self.id);
      let state = self.state.clone();

      if let Some(first) = self.state.record_apis.read().values().nth(0) {
        state.remove_subscription2(first.conn(), id);
        // first.conn().call_and_forget(move |conn| {
        //   state.remove_subscription(conn, id);
        // });
      }
    } else {
      debug!("Subscription cleaned up already by the sender side.");
    }
  }
}

pin_project! {
  /// Receiver wrapper that knows how to cleanup the corresponding subscription.
  #[must_use = "streams do nothing unless polled"]
  pub struct AutoCleanupEventStream {
    state: AutoCleanupEventStreamState,

    #[pin]
    pub receiver: async_channel::Receiver<EventCandidate>,
  }
}
impl AutoCleanupEventStream {
  pub fn new(
    receiver: async_channel::Receiver<EventCandidate>,
    state: Arc<PerConnectionState>,
    id: SubscriptionId,
  ) -> Self {
    return Self {
      state: AutoCleanupEventStreamState {
        receiver: receiver.downgrade(),
        state,
        id,
      },
      receiver,
    };
  }
}

impl Stream for AutoCleanupEventStream {
  type Item = EventCandidate;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let mut this = self.project();
    let res = futures_util::ready!(this.receiver.as_mut().poll_next(cx));
    Poll::Ready(res)
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    self.receiver.size_hint()
  }
}

#[derive(Debug)]
pub struct Subscription {
  /// Id uniquely identifying this subscription.
  pub id: SubscriptionId,
  /// Name of the API this subscription is subscribed to. We need to lookup the Record API on the
  /// hot path to make sure we're getting the latest configuration.
  pub record_api_name: String,
  /// User associated with subscriber.
  pub user: Option<User>,
  /// Record filter.
  pub filter: Filter,
  /// Channel for sending events to the SSE handler.
  pub sender: async_channel::Sender<EventCandidate>,

  pub candidate_seq: AtomicI64,
}

// Represents a change event that needs further filtering, e.g. ACLs.
//
// TODO: Maybe add some sequence number to detect drops if we don't get enough insight from the
// channel.
#[derive(Debug)]
pub struct EventCandidate {
  // TODO: We could proably also just keep the relevant data in the handler rather than passing
  // static subscription metadata over and over again.
  pub subscription: Arc<Subscription>,
  pub record: Option<Arc<indexmap::IndexMap<String, rusqlite::types::Value>>>,
  pub payload: Arc<EventPayload>,
  pub seq: i64,
}

#[derive(Default)]
pub struct Subscriptions {
  /// A list of table subscriptions for this table.
  pub table: Vec<Arc<Subscription>>,

  /// A map of record subscriptions for this.
  pub record: HashMap<i64, Vec<Arc<Subscription>>>,
}

impl Subscriptions {
  fn is_empty(&self) -> bool {
    return self.table.is_empty() && self.record.is_empty();
  }
}

pub struct PerConnectionState {
  /// Metadata: always updated together when config -> record APIs change.
  pub record_apis: RwLock<HashMap<String, RecordApi>>,

  /// Denormalized metadata. We could also grab this from:
  ///   `record_apis.read().nth(0).unwrap().connection_metadata()`.
  pub connection_metadata: RwLock<Arc<ConnectionMetadata>>,

  /// Map from table name to row id to list of subscriptions.
  ///
  /// NOTE: Use layered locking to allow cleaning up per-table subscriptions w/o having to
  /// exclusively lock the entire map.
  pub subscriptions: RwLock<HashMap</* table_name= */ QualifiedName, RwLock<Subscriptions>>>,
}

impl PerConnectionState {
  fn lookup_record_api(&self, name: &str) -> Option<RecordApi> {
    return self.record_apis.read().get(name).cloned();
  }

  // Gets called by the Stream destructor, e.g. when a client disconnects.
  // pub fn remove_subscription(&self, conn: &rusqlite::Connection, id: SubscriptionId) {
  //   let mut read_lock = self.subscriptions.upgradable_read();
  //
  //   let remove_subscription_entry_for_table = {
  //     let Some(mut subscriptions) = read_lock.get(&id.table_name).map(|l| l.write()) else {
  //       return;
  //     };
  //
  //     if let Some(row_id) = id.row_id {
  //       if let Some(record_subscriptions) = subscriptions.record.get_mut(&row_id) {
  //         record_subscriptions.retain(|sub| {
  //           return sub.id.sub_id != id.sub_id;
  //         });
  //
  //         if record_subscriptions.is_empty() {
  //           subscriptions.record.remove(&row_id);
  //         }
  //       }
  //     } else {
  //       subscriptions.table.retain(|sub| {
  //         return sub.id.sub_id != id.sub_id;
  //       });
  //     }
  //
  //     subscriptions.is_empty()
  //   };
  //
  //   if remove_subscription_entry_for_table {
  //     let table_name = &id.table_name;
  //     // NOTE: Only write lock across all tables when necessary.
  //     read_lock.with_upgraded(|lock| {
  //       // Check again to avoid races:
  //       if lock.get(table_name).is_some_and(|e| e.read().is_empty()) {
  //         lock.remove(table_name);
  //
  //         if lock.is_empty() {
  //           uninstall_hook_rusqlite(conn);
  //         }
  //       }
  //     });
  //   }
  // }

  pub fn remove_subscription2(&self, conn: &trailbase_sqlite::Connection, id: SubscriptionId) {
    let mut read_lock = self.subscriptions.upgradable_read();

    let remove_subscription_entry_for_table = {
      let Some(mut subscriptions) = read_lock.get(&id.table_name).map(|l| l.write()) else {
        return;
      };

      if let Some(row_id) = id.row_id {
        if let Some(record_subscriptions) = subscriptions.record.get_mut(&row_id) {
          record_subscriptions.retain(|sub| {
            return sub.id.sub_id != id.sub_id;
          });

          if record_subscriptions.is_empty() {
            subscriptions.record.remove(&row_id);
          }
        }
      } else {
        subscriptions.table.retain(|sub| {
          return sub.id.sub_id != id.sub_id;
        });
      }

      subscriptions.is_empty()
    };

    if remove_subscription_entry_for_table {
      let table_name = &id.table_name;
      // NOTE: Only write lock across all tables when necessary.
      read_lock.with_upgraded(|lock| {
        // Check again to avoid races:
        if lock.get(table_name).is_some_and(|e| e.read().is_empty()) {
          lock.remove(table_name);

          if lock.is_empty() {
            uninstall_hook(conn);
          }
        }
      });
    }
  }

  fn add_hook(self: &Arc<Self>, api: RecordApi) {
    let conn = (**api.conn()).clone();
    let state = self.clone();

    let receiver = install_hook(&conn).to_async();

    tokio::spawn(async move {
      let mut expected = 1;
      loop {
        if receiver.sender_count() == 0 {
          break;
        }

        let event = match receiver.recv().await {
          Ok((cnt, event)) => {
            if cnt != expected {
              // QUESTION: There's several ways we could deal with failure. We
              // probably shouldn't create back pressure on the preupdate_hook and gunk up the
              // SQLite access. We could try to deliver event loss messages to all receivers but
              // that may just make the probem worse. We're probably at limit already
              // if we don't manage to catch up. Should we just disconnect all subscriptions?
              let mut all_subscriptions = state.subscriptions.write();
              all_subscriptions.clear();
              break;
            }
            expected += 1;

            event
          }
          Err(kanal::ReceiveError::Closed) | Err(kanal::ReceiveError::SendClosed) => {
            break;
          }
        };

        broker(&conn, &state, event).await;
      }

      debug!("Channel closed: terminating subscription broker task.");
    });
  }

  pub async fn add_record_subscription(
    self: Arc<Self>,
    api: RecordApi,
    record: trailbase_sqlite::Value,
    user: Option<User>,
    sender: async_channel::Sender<EventCandidate>,
  ) -> Result<Arc<Subscription>, RecordError> {
    let table_name = api.table_name();
    let pk_column = &api.record_pk_column().column.name;

    let Some(row_id): Option<i64> = api
      .conn()
      .read_query_row_f(
        format!(r#"SELECT _rowid_ FROM {table_name} WHERE "{pk_column}" = $1"#),
        [record],
        |row| row.get(0),
      )
      .await?
    else {
      return Err(RecordError::RecordNotFound);
    };

    let qualified_name = api.qualified_name();
    let subscription_entry = Arc::new(Subscription {
      id: SubscriptionId {
        table_name: qualified_name.clone(),
        row_id: Some(row_id),
        sub_id: SUBSCRIPTION_COUNTER.fetch_add(1, Ordering::SeqCst),
      },
      record_api_name: api.api_name().to_string(),
      user,
      sender,
      filter: Filter::Passthrough,
      candidate_seq: AtomicI64::default(),
    });

    let install_hook: bool = {
      let mut lock = self.subscriptions.write();
      let empty = lock.is_empty();

      let subscriptions = lock.entry(qualified_name.clone()).or_default();
      subscriptions
        .write()
        .record
        .entry(row_id)
        .or_default()
        .push(subscription_entry.clone());

      empty
    };

    if install_hook {
      self.add_hook(api.clone());
    }

    return Ok(subscription_entry);
  }

  pub async fn add_table_subscription(
    self: Arc<Self>,
    api: RecordApi,
    user: Option<User>,
    filter: Option<ValueOrComposite>,
    sender: async_channel::Sender<EventCandidate>,
  ) -> Result<Arc<Subscription>, RecordError> {
    let filter = if let Some(filter) = filter {
      Filter::Record(qs_filter_to_record_filter(api.columns(), filter)?)
    } else {
      Filter::Passthrough
    };

    let qualified_name = api.qualified_name();
    let subscription_entry = Arc::new(Subscription {
      id: SubscriptionId {
        table_name: qualified_name.clone(),
        row_id: None,
        sub_id: SUBSCRIPTION_COUNTER.fetch_add(1, Ordering::SeqCst),
      },
      record_api_name: api.api_name().to_string(),
      user,
      sender,
      filter,
      candidate_seq: AtomicI64::default(),
    });

    let install_hook: bool = {
      let mut lock = self.subscriptions.write();
      let empty = lock.is_empty();

      let subscriptions = lock.entry(qualified_name.clone()).or_default();
      subscriptions.write().table.push(subscription_entry.clone());

      empty
    };

    if install_hook {
      self.add_hook(api.clone());
    }

    return Ok(subscription_entry);
  }
}

impl Drop for PerConnectionState {
  fn drop(&mut self) {
    if let Some(first) = self.record_apis.read().values().nth(0) {
      uninstall_hook(first.conn());
    }
  }
}

async fn broker_subscriptions(
  state: &Arc<PerConnectionState>,
  conn: &trailbase_sqlite::Connection,
  subs: &[Arc<Subscription>],
  record: &Arc<indexmap::IndexMap<String, rusqlite::types::Value>>,
  event: &Arc<EventPayload>,
) {
  let state = state.clone();
  let conn = conn.clone();

  futures_util::future::join_all(subs.iter().map(move |sub| {
    // Cloning the event. It's important that we use a try_send here to not block other
    // subscriptions if a subscriber is slow and their channel fills up.
    if let Err(err) = sub.sender.try_send(EventCandidate {
      subscription: sub.clone(),
      record: Some(record.clone()),
      payload: event.clone(),
      seq: sub.candidate_seq.fetch_add(1, Ordering::SeqCst),
    }) {
      match err {
        async_channel::TrySendError::Full(ev) => {
          debug!("Channel full, dropping event: {ev:?}");
        }
        async_channel::TrySendError::Closed(_ev) => {
          state.remove_subscription2(&conn, sub.id.clone());
        }
      }
    }

    return async { () };
  }))
  .await;
}

/// Broker event to various subscriptions.
async fn broker(
  conn: &trailbase_sqlite::Connection,
  state: &Arc<PerConnectionState>,
  event: PreupdateHookEvent,
) {
  let PreupdateHookEvent {
    action,
    table_name,
    row_id,
    record,
  } = event;

  let mut per_table_subscriptions = state.subscriptions.upgradable_read();

  // If table_metadata is missing, the config/schema must have changed, thus removing the
  // subscriptions.
  let connection_metadata_lock = state.connection_metadata.read();
  let Some(table_metadata) = connection_metadata_lock.get_table(&table_name) else {
    warn!("Table {table_name:?} not found. Removing subscriptions");

    per_table_subscriptions.with_upgraded(|lock| {
      lock.remove(&table_name);

      if lock.is_empty() {
        uninstall_hook(conn);
      }
    });

    return;
  };

  // Check if there are any matching subscriptions and otherwise go back to listening.
  let Some(subscriptions) = per_table_subscriptions.get(&table_name).map(|r| r.read()) else {
    return;
  };
  if subscriptions.table.is_empty() && !subscriptions.record.contains_key(&row_id) {
    return;
  }

  // Join values with column names. We use a map rather than a Vec<(String, Value)> for filter
  // access.
  let record: Arc<indexmap::IndexMap<String, rusqlite::types::Value>> = Arc::new(
    record
      .into_iter()
      .enumerate()
      .map(|(idx, v)| (table_metadata.schema.columns[idx].name.clone(), v))
      .collect(),
  );

  // Build a JSON-encoded SQLite event (insert, update, delete).
  let event: Arc<EventPayload> = {
    let json_obj = record
      .iter()
      .filter_map(|(name, value)| {
        return value_to_flat_json(value)
          .ok()
          .map(|v| (name.to_string(), v));
      })
      .collect();

    Arc::new(EventPayload::from(&match action {
      RecordAction::Delete => JsonEventPayload::Delete { value: json_obj },
      RecordAction::Insert => JsonEventPayload::Insert { value: json_obj },
      RecordAction::Update => JsonEventPayload::Update { value: json_obj },
    }))
  };

  // First broker record subscriptions.
  if let Some(record_subscriptions) = subscriptions.record.get(&row_id) {
    broker_subscriptions(state, conn, record_subscriptions, &record, &event).await
  }

  // Then broker table subscriptions.
  broker_subscriptions(state, conn, &subscriptions.table, &record, &event).await;
}

static SUBSCRIPTION_COUNTER: AtomicI64 = AtomicI64::new(0);
