use async_channel::WeakReceiver;
use futures_util::Stream;
use log::*;
use parking_lot::Mutex;
use pin_project_lite::pin_project;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Weak};
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
  state: Weak<PerConnectionState>,
  id: SubscriptionId,
}

impl Drop for AutoCleanupEventStreamState {
  fn drop(&mut self) {
    // Subscriptions can be cleaned up either by the sender, i.e. when trying to broker events and
    // tables or records get deleted, or by the client-receiver, e.g. by disconnecting.
    // When dropped by the client-side, we need to clean up the subscription.
    if self.receiver.upgrade().is_some() {
      let id = std::mem::take(&mut self.id);

      let Some(state) = self.state.upgrade() else {
        return;
      };

      state.state.lock().remove_subscription2(id);
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
        state: Arc::downgrade(&state),
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

pub struct PerConnectionStateInternal {
  /// Metadata: always updated together when config -> record APIs change.
  pub record_apis: HashMap<String, RecordApi>,

  /// Denormalized metadata. We could also grab this from:
  ///   `record_apis.read().nth(0).unwrap().connection_metadata()`.
  pub connection_metadata: Arc<ConnectionMetadata>,

  /// Should be the same as for all `record_apis` above.
  pub conn: Arc<trailbase_sqlite::Connection>,

  /// Map from table name to row id to list of subscriptions.
  ///
  /// NOTE: Use layered locking to allow cleaning up per-table subscriptions w/o having to
  /// exclusively lock the entire map.
  pub subscriptions: HashMap</* table_name= */ QualifiedName, Subscriptions>,
}

impl PerConnectionStateInternal {
  pub fn remove_subscription2(&mut self, id: SubscriptionId) {
    let Some(subscriptions) = self.subscriptions.get_mut(&id.table_name) else {
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

    if subscriptions.is_empty() {
      self.subscriptions.remove(&id.table_name);
      if self.subscriptions.is_empty() {
        uninstall_hook(&self.conn);
      }
    }
  }
}

pub struct PerConnectionState {
  pub state: Mutex<PerConnectionStateInternal>,
}

impl PerConnectionState {
  fn add_hook(self: &Arc<Self>, api: RecordApi) {
    let conn = api.conn().clone();
    let state = self.clone();

    let receiver = install_hook(&conn).to_async();

    // Spawn broker task.
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
              // that may just make the problem worse. We're probably at limit already
              // if we don't manage to catch up. Should we just disconnect all subscriptions?
              state.state.lock().subscriptions.clear();
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
      let mut lock = self.state.lock();
      let empty = lock.subscriptions.is_empty();

      let subscriptions = lock
        .subscriptions
        .entry(qualified_name.clone())
        .or_default();
      subscriptions
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
      let mut lock = self.state.lock();
      let empty = lock.subscriptions.is_empty();

      let subscriptions = lock
        .subscriptions
        .entry(qualified_name.clone())
        .or_default();

      subscriptions.table.push(subscription_entry.clone());

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
    uninstall_hook(&self.state.lock().conn);
  }
}

async fn broker_subscriptions(
  subs: &[Arc<Subscription>],
  record: &Arc<indexmap::IndexMap<String, rusqlite::types::Value>>,
  event: &Arc<EventPayload>,
) -> Vec<Option<usize>> {
  return futures_util::future::join_all(subs.iter().enumerate().map(async move |(idx, sub)| {
    // Cloning the event. It's important that we use a try_send here to not block other
    // subscriptions if a subscriber is slow and their channel fills up.
    if let Err(err) = sub.sender.try_send(EventCandidate {
      record: Some(record.clone()),
      payload: event.clone(),
      seq: sub.candidate_seq.fetch_add(1, Ordering::SeqCst),
    }) {
      match err {
        async_channel::TrySendError::Full(ev) => {
          debug!("Channel full, dropping event: {ev:?}");
        }
        async_channel::TrySendError::Closed(_ev) => {
          return Some(idx);
        }
      }
    };

    return None;
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

  let mut state = state.state.lock();

  // If table_metadata is missing, the config/schema must have changed, thus removing the
  // subscriptions.
  let connection_metadata = state.connection_metadata.clone();
  let Some(table_metadata) = connection_metadata.get_table(&table_name) else {
    warn!("Table {table_name:?} not found. Removing subscriptions");

    state.subscriptions.remove(&table_name);
    if state.subscriptions.is_empty() {
      uninstall_hook(conn);
    }
    return;
  };

  // Check if there are any matching subscriptions and otherwise go back to listening.
  let Some(subscriptions) = state.subscriptions.get_mut(&table_name) else {
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

  // FIXME: Holding lock across wait points. We could:
  // * Ignore it
  // * Use tokio::sync::Mutex
  // * Use a dedicated thread and just block to avoid accidentally blocking shared worker threads.

  // First broker record subscriptions.
  if let Some(record_subscriptions) = subscriptions.record.get_mut(&row_id) {
    let dead = broker_subscriptions(record_subscriptions, &record, &event).await;

    for idx in dead.iter().rev().flatten() {
      record_subscriptions.remove(*idx);
    }
  }

  // Then broker table subscriptions.
  let dead = broker_subscriptions(&subscriptions.table, &record, &event).await;
  for idx in dead.iter().rev() {
    if let Some(idx) = idx {
      subscriptions.table.remove(*idx);
    }
  }

  if subscriptions.is_empty() {
    uninstall_hook(conn);
  }
}

static SUBSCRIPTION_COUNTER: AtomicI64 = AtomicI64::new(0);
