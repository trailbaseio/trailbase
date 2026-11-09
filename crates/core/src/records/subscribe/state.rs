use async_channel::WeakReceiver;
use futures_util::Stream;
use log::*;
use parking_lot::Mutex;
use pin_project_lite::pin_project;
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Weak};
use std::task::{Context, Poll};
use trailbase_qs::ValueOrComposite;
use trailbase_schema::QualifiedName;
use trailbase_schema::json::record_to_json_expand;

use crate::auth::User;
use crate::records::filter::{Filter, qs_filter_to_record_filter};
use crate::records::subscribe::event::{EventError, EventPayload, Record};
use crate::records::subscribe::hook::{
  BrokerEvent, CommittedEvent, HookHandle, RecordAction, WeakHookHandle,
};
use crate::records::{RecordApi, RecordApiByName, RecordError};
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
  state: Weak<Mutex<PerConnectionStateInternal>>,
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

      state.lock().remove_subscription(&id);
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
    state: &PerConnectionState,
    id: SubscriptionId,
  ) -> Self {
    return Self {
      state: AutoCleanupEventStreamState {
        receiver: receiver.downgrade(),
        state: Arc::downgrade(&state.state),
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

  /// Keeps the SQLite connection hooks alive.
  pub _handle: HookHandle,
}

// Represents a change event that needs further filtering, e.g. ACLs.
#[derive(Debug)]
pub struct EventCandidate {
  pub payload: Arc<EventPayload>,
  // Sequence number is counted by the central broker thread to detect event loss due to
  // overflowing buffers.
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

  fn shutdown(&mut self) {
    for sub in std::mem::take(&mut self.table) {
      sub.sender.close();
    }

    for (_id, subs) in std::mem::take(&mut self.record) {
      for sub in subs {
        sub.sender.close();
      }
    }
  }
}

struct PerConnectionStateInternal {
  /// Denormalized metadata. We could also grab this from:
  ///   `record_apis.read().nth(0).unwrap().connection_metadata()`.
  connection_metadata: Arc<ConnectionMetadata>,

  /// Metadata: always updated together when config -> record APIs change.
  record_apis: HashSet<RecordApiByName>,

  /// Map from table name to row id to list of subscriptions.
  ///
  /// NOTE: Use layered locking to allow cleaning up per-table subscriptions w/o having to
  /// exclusively lock the entire map.
  subscriptions: HashMap</* table_name= */ QualifiedName, Subscriptions>,

  /// Just a dead-subscriptions buffer to reduce allocations.
  buffer: Vec<usize>,
}

impl PerConnectionStateInternal {
  fn remove_subscription(&mut self, id: &SubscriptionId) {
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
    }
  }
}

#[derive(Clone)]
pub struct PerConnectionState {
  state: Arc<Mutex<PerConnectionStateInternal>>,
  /// Manages the installed hooks for the associated connection. Kept alive by subscriptions above.
  weak_hook_handle: Arc<WeakHookHandle>,
}

impl PerConnectionState {
  pub fn new(
    conn: trailbase_sqlite::Connection,
    connection_metadata: Arc<ConnectionMetadata>,
    record_apis: HashSet<RecordApiByName>,
  ) -> Self {
    let state = Arc::new(Mutex::new(PerConnectionStateInternal {
      connection_metadata,
      record_apis,
      subscriptions: Default::default(),
      buffer: Default::default(),
    }));

    let weak_hook_handle = Arc::new(WeakHookHandle::new(conn, {
      let state = state.clone();

      move |event| match event {
        BrokerEvent::EventLoss => {
          state.lock().subscriptions.clear();
        }
        BrokerEvent::Event(event) => {
          broker(&mut state.lock(), event);
        }
      }
    }));

    return Self {
      state,
      weak_hook_handle,
    };
  }

  pub fn update(
    &self,
    connection_metadata: Arc<ConnectionMetadata>,
    record_apis: HashSet<RecordApiByName>,
  ) {
    let mut lock = self.state.lock();
    lock.connection_metadata = connection_metadata;
    lock.record_apis = record_apis;
  }

  pub fn shutdown(&self) {
    let mut state = self.state.lock();

    for subs in std::mem::take(&mut state.subscriptions).values_mut() {
      subs.shutdown();
    }
  }

  pub async fn add_record_subscription(
    &self,
    api: RecordApi,
    record: trailbase_sqlite::Value,
    user: Option<User>,
    sender: async_channel::Sender<EventCandidate>,
  ) -> Result<Arc<Subscription>, RecordError> {
    async fn get_row_id(
      api: &RecordApi,
      record: trailbase_sqlite::Value,
    ) -> Result<i64, RecordError> {
      // NOTE: It's important that the handler already did ACL checking to not allow unprivileged
      // record enumerations.
      return api
        .conn()
        .read_query_row_get(
          format!(
            "SELECT _rowid_ FROM {table_name} WHERE \"{pk_column}\" = $1",
            table_name = api.table_name(),
            pk_column = api.record_pk_column().column.name,
          ),
          (record,),
          0,
        )
        .await
        .map_err(|err| RecordError::Internal(err.into()))?
        .ok_or(RecordError::RecordNotFound);
    }

    // First check that the requested record exists.
    let row_id = get_row_id(&api, record.clone()).await?;

    let qualified_name = api.qualified_name().clone();
    let id = SubscriptionId {
      table_name: qualified_name.clone(),
      row_id: Some(row_id),
      sub_id: SUBSCRIPTION_COUNTER.fetch_add(1, Ordering::SeqCst),
    };

    let subscription_entry = Arc::new(Subscription {
      id: id.clone(),
      record_api_name: api.api_name().to_string(),
      user,
      sender,
      filter: Filter::Passthrough,
      candidate_seq: AtomicI64::default(),
      _handle: self.weak_hook_handle.handle()?,
    });

    self
      .state
      .lock()
      .subscriptions
      .entry(qualified_name)
      .or_default()
      .record
      .entry(row_id)
      .or_default()
      .push(subscription_entry.clone());

    // Finally check again that the record still exists to avoid racing concurrent record deletion.
    // Otherwise, we may have installed a hook but missed the deletion event, thus orphaning a
    // subscription.
    if let Err(_err) = get_row_id(&api, record).await {
      self.state.lock().remove_subscription(&id);
    }

    return Ok(subscription_entry);
  }

  pub async fn add_table_subscription(
    &self,
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

    let qualified_name = api.qualified_name().clone();

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
      _handle: self.weak_hook_handle.handle()?,
    });

    self
      .state
      .lock()
      .subscriptions
      .entry(qualified_name)
      .or_default()
      .table
      .push(subscription_entry.clone());

    return Ok(subscription_entry);
  }

  #[cfg(test)]
  pub fn num_record_subscriptions(&self) -> usize {
    let mut count: usize = 0;
    for (_table_name, subs) in self.state.lock().subscriptions.iter() {
      for record in subs.record.values() {
        count += record.len();
      }
    }
    return count;
  }

  #[cfg(test)]
  pub fn num_table_subscriptions(&self) -> usize {
    let mut count: usize = 0;
    for (_table_name, subs) in self.state.lock().subscriptions.iter() {
      count += subs.table.len();
    }
    return count;
  }
}

#[inline]
fn broker_subscriptions(
  subs: &[Arc<Subscription>],
  payload: &Arc<EventPayload>,
  dead_subscriptions: &mut Vec<usize>,
) {
  for (idx, sub) in subs.iter().enumerate() {
    // Cloning the event. It's important that we use a try_send here to not block other
    // subscriptions if a subscriber is slow and their channel fills up.
    if let Err(err) = sub.sender.try_send(EventCandidate {
      payload: payload.clone(),
      seq: sub.candidate_seq.fetch_add(1, Ordering::SeqCst),
    }) {
      match err {
        async_channel::TrySendError::Full(ev) => {
          debug!("Channel full, dropping event: {ev:?}");
        }
        async_channel::TrySendError::Closed(_ev) => {
          dead_subscriptions.push(idx);
        }
      }
    };
  }
}

/// Broker event to various subscriptions.
fn broker(state: &mut PerConnectionStateInternal, event: CommittedEvent) {
  let CommittedEvent {
    action,
    table_name,
    row_id,
    record,
  } = event;

  // If table_metadata is missing, the config/schema must have changed, thus removing the
  // subscriptions.
  let Some(table_metadata) = state.connection_metadata.get_table(&table_name) else {
    warn!("Table {table_name:?} not found. Removing subscriptions");

    state.subscriptions.remove(&table_name);
    return;
  };

  // Check if there are any matching subscriptions and otherwise go back to listening.
  let Some(subscriptions) = state.subscriptions.get_mut(&table_name) else {
    return;
  };
  let record_subscriptions = subscriptions.record.get_mut(&row_id);
  if subscriptions.table.is_empty() && record_subscriptions.is_none() {
    return;
  }

  // Build a JSON-encoded SQLite event (insert, update, delete).
  let record_and_event: Arc<EventPayload> = {
    // Join values with column names. We need index access for `record_to_json_expanded` below and
    // column-name-based access for filters, we thus use an IndexMap rather than a Vec<(String,
    // Value)> a data type. Needs to be an Arc so it can be passed to sqlite worker across
    // async boundary.
    let record: Record = record
      .into_iter()
      .enumerate()
      .map(|(idx, v)| (table_metadata.schema.columns[idx].name.clone(), v))
      .collect();

    match record_to_json_expand(&table_metadata.column_metadata, &[], &record, None) {
      Ok(json_obj) => Arc::new(match action {
        RecordAction::Insert => EventPayload::insert(json_obj, record),
        RecordAction::Update => EventPayload::update(json_obj, record),
        RecordAction::Delete => EventPayload::delete(json_obj, record),
      }),
      Err(err) => Arc::new(EventPayload::error(&EventError {
        status: super::event::EventErrorStatus::Serialization,
        message: if cfg!(debug_assertions) {
          Some(err.to_string())
        } else {
          None
        },
      })),
    }
  };

  // NOTE: If/when latency becomes an issue we could do all the brokering (both records & tables as
  // well as across subscriptions) in parallel, however it's already a non-blocking buffered send.

  // First broker record subscriptions.
  let dead_subscriptions = &mut state.buffer;
  if let Some(record_subscriptions) = subscriptions.record.get_mut(&row_id) {
    debug_assert!(
      !record_subscriptions.is_empty(),
      "should have been cleaned up"
    );

    dead_subscriptions.clear();
    broker_subscriptions(record_subscriptions, &record_and_event, dead_subscriptions);

    for idx in dead_subscriptions.iter().rev() {
      record_subscriptions.remove(*idx);
    }
  }

  // Then broker table subscriptions.
  if !subscriptions.table.is_empty() {
    dead_subscriptions.clear();
    broker_subscriptions(&subscriptions.table, &record_and_event, dead_subscriptions);

    for idx in dead_subscriptions.iter().rev() {
      subscriptions.table.remove(*idx);
    }
  }
}

static SUBSCRIPTION_COUNTER: AtomicI64 = AtomicI64::new(0);
