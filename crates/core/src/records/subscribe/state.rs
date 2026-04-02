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
use crate::records::filter::{
  Filter, apply_filter_recursively_to_record, qs_filter_to_record_filter,
};
use crate::records::record_api::SubscriptionAclParams;
use crate::records::subscribe::event::{EventPayload, JsonEventPayload};
use crate::records::subscribe::hook::{
  PreupdateHookEvent, RecordAction, install_hook, uninstall_hook, uninstall_hook_rusqlite,
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
        first.conn().call_and_forget(move |conn| {
          state.remove_subscription(conn, id);
        });
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
  fn remove_subscription(&self, conn: &rusqlite::Connection, id: SubscriptionId) {
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
            uninstall_hook_rusqlite(conn);
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
      loop {
        if receiver.sender_count() == 0 {
          break;
        }

        let event = match receiver.recv().await {
          Ok(event) => event,
          Err(kanal::ReceiveError::Closed) | Err(kanal::ReceiveError::SendClosed) => {
            break;
          }
        };

        let state = state.clone();
        broker_event(conn.clone(), state, event).await;
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

// fn broker_subscriptions(
//   s: &PerConnectionState,
//   conn: &rusqlite::Connection,
//   subs: &[Arc<Subscription>],
//   record_subscriptions: bool,
//   record: &Arc<indexmap::IndexMap<String, rusqlite::types::Value>>,
//   event: &Arc<EventPayload>,
// ) -> Vec<usize> {
//   let mut dead_subscriptions: Vec<usize> = vec![];
//
//   for (idx, sub) in subs.iter().enumerate() {
//     // Skip events for records that are being filtered out anyway.
//     if let Filter::Record(ref filter) = sub.filter
//       && !apply_filter_recursively_to_record(filter, record)
//     {
//       continue;
//     }
//
//     // We don't memoize and eagerly look up the APIs to make sure we get an up-to-date version.
//     let Some(api) = s.lookup_record_api(&sub.record_api_name) else {
//       dead_subscriptions.push(idx);
//       sub.sender.close();
//       continue;
//     };
//
//     if let Err(_err) = api.check_record_level_read_access_for_subscriptions(
//       conn,
//       SubscriptionAclParams {
//         params: record,
//         user: sub.user.as_ref(),
//       },
//     ) {
//       // NOTE: that access failures for table subscriptions for specific records are simply ignored,
//       // i.e. those events will just not be send. Other records in the table may pass the
//       // check. For record subscriptions, however, missing access is a death sentence.
//       if record_subscriptions {
//         // This can happen if the record api configuration has changed since originally
//         // subscribed. In this case we just send and error and cancel the subscription.
//         match sub.sender.try_send(EventCandidate {
//           record: None,
//           payload: ACCESS_DENIED_EVENT.clone(),
//         }) {
//           Ok(_) | Err(TrySendError::Full(_)) => {
//             sub.sender.close();
//           }
//           Err(TrySendError::Closed(_)) => {}
//         };
//
//         dead_subscriptions.push(idx);
//       }
//       continue;
//     }
//
//     // Cloning the event. It's important that we use a try_send here to not block other
//     // subscriptions if a subscriber is slow and their channel fills up.
//     if let Err(err) = sub.sender.try_send(EventCandidate {
//       record: Some(record.clone()),
//       payload: event.clone(),
//     }) {
//       match err {
//         async_channel::TrySendError::Full(ev) => {
//           debug!("Channel full, dropping event: {ev:?}");
//         }
//         async_channel::TrySendError::Closed(_ev) => {
//           dead_subscriptions.push(idx);
//           sub.sender.close();
//         }
//       }
//     }
//   }
//
//   return dead_subscriptions;
// }

async fn broker_subscriptions2(
  subs: &[Arc<Subscription>],
  record: &Arc<indexmap::IndexMap<String, rusqlite::types::Value>>,
  event: &Arc<EventPayload>,
) -> Vec<usize> {
  let mut dead_subscriptions: Vec<usize> = vec![];

  futures_util::future::join_all(subs.iter().enumerate().map(async |(idx, sub)| {
    // Cloning the event. It's important that we use a try_send here to not block other
    // subscriptions if a subscriber is slow and their channel fills up.
    if let Err(err) = sub.sender.try_send(EventCandidate {
      subscription: sub.clone(),
      record: Some(record.clone()),
      payload: event.clone(),
    }) {
      match err {
        async_channel::TrySendError::Full(ev) => {
          debug!("Channel full, dropping event: {ev:?}");
        }
        async_channel::TrySendError::Closed(_ev) => {
          // dead_subscriptions.push(idx);
          // sub.sender.close();
        }
      }
    }

    return ();
  }))
  .await;

  return dead_subscriptions;
}

/// Broker event to various subscriptions.
async fn broker_event(
  conn: trailbase_sqlite::Connection,
  state: Arc<PerConnectionState>,
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
        uninstall_hook(&conn);
      }
    });

    return;
  };

  let remove_subscription_entry_for_table = {
    // Check if there are any matching subscriptions and otherwise go back to listening.
    let Some(mut subscriptions) = per_table_subscriptions
      .get(&table_name)
      .map(|r| r.upgradable_read())
    else {
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

      let payload = EventPayload::from(&match action {
        RecordAction::Delete => JsonEventPayload::Delete { value: json_obj },
        RecordAction::Insert => JsonEventPayload::Insert { value: json_obj },
        RecordAction::Update => JsonEventPayload::Update { value: json_obj },
      });

      Arc::new(payload)
    };

    // First broker record subscriptions.
    let (dead_record_subscriptions, dead_table_subscriptions) = {
      let dead_record_subscriptions =
        if let Some(record_subscriptions) = subscriptions.record.get(&row_id) {
          broker_subscriptions2(record_subscriptions, &record, &event).await
        } else {
          vec![]
        };

      // Then broker table subscriptions.
      let dead_table_subscriptions =
        broker_subscriptions2(&subscriptions.table, &record, &event).await;

      (dead_record_subscriptions, dead_table_subscriptions)
    };

    if dead_record_subscriptions.is_empty()
      && dead_table_subscriptions.is_empty()
      && action != RecordAction::Delete
    {
      // No cleanup needed.
      return;
    }

    subscriptions.with_upgraded(|subscriptions| {
      // Record subscription cleanup.
      match action {
        RecordAction::Delete => {
          // This is unique for record subscriptions: if the record is deleted, cancel all
          // subscriptions.
          subscriptions.record.remove(&row_id);
        }
        RecordAction::Update | RecordAction::Insert => {
          if let Some(m) = subscriptions.record.get_mut(&row_id) {
            for idx in dead_record_subscriptions.iter().rev() {
              m.swap_remove(*idx);
            }

            if m.is_empty() {
              subscriptions.record.remove(&row_id);
            }
          }
        }
      }

      // Table subscription cleanup.
      for idx in dead_table_subscriptions.iter().rev() {
        subscriptions.table.swap_remove(*idx);
      }

      /* remove_subscription_entry_for_table = */
      subscriptions.is_empty()
    })
  };

  if remove_subscription_entry_for_table {
    // NOTE: Only write lock across all tables when necessary.
    per_table_subscriptions.with_upgraded(|lock| {
      // Check again to avoid races:
      if lock.get(&table_name).is_some_and(|e| e.read().is_empty()) {
        lock.remove(&table_name);

        if lock.is_empty() {
          uninstall_hook(&conn);
        }
      }
    });
  }
}

static SUBSCRIPTION_COUNTER: AtomicI64 = AtomicI64::new(0);

static ACCESS_DENIED_EVENT: LazyLock<Arc<EventPayload>> = LazyLock::new(|| {
  Arc::new(EventPayload::from(&JsonEventPayload::Error {
    error: "Access denied".into(),
  }))
});

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn static_sse_event_test() {
    let _x: Arc<EventPayload> = (*ACCESS_DENIED_EVENT).clone();
  }
}
