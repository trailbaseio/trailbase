use async_channel::{TrySendError, WeakReceiver};
use futures_util::Stream;
use log::*;
use parking_lot::RwLock;
use pin_project_lite::pin_project;
use reactivate::Reactive;
use rusqlite::hooks::{Action, PreUpdateCase};
use serde::Serialize;
use std::collections::{HashMap, hash_map::Entry};
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, LazyLock};
use std::task::{Context, Poll};
use trailbase_qs::ValueOrComposite;
use trailbase_schema::QualifiedName;
use trailbase_schema::json::value_to_flat_json;
use trailbase_sqlite::connection::{extract_record_values, extract_row_id};

use crate::app_state::derive_unchecked;
use crate::auth::User;
use crate::records::RecordApi;
use crate::records::RecordError;
use crate::records::filter::{
  Filter, apply_filter_recursively_to_record, qs_filter_to_record_filter,
};
use crate::records::record_api::SubscriptionAclParams;
use crate::records::subscribe::event::{EventPayload, JsonEventPayload};
use crate::schema_metadata::ConnectionMetadata;

/// Composite id uniquely identifying a subscription.
///
/// If row_id is Some, this is considered to reference a subscription to a specific record.
#[derive(Default, Debug)]
struct SubscriptionId {
  table_name: QualifiedName,
  sub_id: i64,
  row_id: Option<i64>,
}

/// RAII type for automatically cleaning up subscriptions when the receiving side gets dropped,
/// e.g. client disconnects.
struct AutoCleanupEventStreamState {
  receiver: WeakReceiver<Arc<EventPayload>>,
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
    pub receiver: async_channel::Receiver<Arc<EventPayload>>,
  }
}

impl Stream for AutoCleanupEventStream {
  type Item = Arc<EventPayload>;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let mut this = self.project();
    let res = futures_util::ready!(this.receiver.as_mut().poll_next(cx));
    Poll::Ready(res)
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    self.receiver.size_hint()
  }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize)]
pub enum RecordAction {
  Delete,
  Insert,
  Update,
}

pub struct Subscription {
  /// Id uniquely identifying this subscription.
  subscription_id: i64,
  /// Name of the API this subscription is subscribed to. We need to lookup the Record API on the
  /// hot path to make sure we're getting the latest configuration.
  record_api_name: String,
  /// User associated with subscriber.
  user: Option<User>,
  /// Channel for sending events to the SSE handler.
  sender: async_channel::Sender<Arc<EventPayload>>,
  /// Filter
  filter: Filter,
}

#[derive(Default)]
struct Subscriptions {
  /// A list of table subscriptions for this table.
  table: Vec<Subscription>,

  /// A map of record subscriptions for this.
  record: HashMap<i64, Vec<Subscription>>,
}

impl Subscriptions {
  fn is_empty(&self) -> bool {
    return self.table.is_empty() && self.record.is_empty();
  }
}

struct PerConnectionState {
  /// Metadata: always updated together when config -> record APIs change.
  record_apis: RwLock<HashMap<String, RecordApi>>,
  /// Denormalized metadata. We could also grab this from:
  ///   `record_apis.read().nth(0).unwrap().connection_metadata()`.
  connection_metadata: RwLock<Arc<ConnectionMetadata>>,

  /// Map from table name to row id to list of subscriptions.
  ///
  /// NOTE: Use layered locking to allow cleaning up per-table subscriptions w/o having to
  /// exclusively lock the entire map.
  subscriptions: RwLock<HashMap</* table_name= */ QualifiedName, RwLock<Subscriptions>>>,
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
            return sub.subscription_id != id.sub_id;
          });

          if record_subscriptions.is_empty() {
            subscriptions.record.remove(&row_id);
          }
        }
      } else {
        subscriptions.table.retain(|sub| {
          return sub.subscription_id != id.sub_id;
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
            conn.preupdate_hook(NO_HOOK).expect("owned conn");
          }
        }
      });
    }
  }

  fn add_hook(self: &Arc<Self>, api: RecordApi) {
    let conn = api.conn().clone();
    let state = self.clone();

    api
      .conn()
      .write_lock()
      .preupdate_hook(Some(
        move |action: Action, db: &str, table_name: &str, case: &PreUpdateCase| {
          let action = match action {
            Action::SQLITE_UPDATE => RecordAction::Update,
            Action::SQLITE_INSERT => RecordAction::Insert,
            Action::SQLITE_DELETE => RecordAction::Delete,
            a => {
              warn!("Skipping unknown action: {a:?}");
              return;
            }
          };

          let Some(rowid) = extract_row_id(case) else {
            warn!("Failed to extract row id");
            return;
          };

          let qualified_table_name = QualifiedName {
            name: table_name.to_string(),
            database_schema: Some(db.to_string()),
          };

          // If there are no matching subscriptions, skip.
          {
            let lock = state.subscriptions.read();
            let Some(subscriptions) = lock.get(&qualified_table_name).map(|r| r.read()) else {
              return;
            };

            if subscriptions.table.is_empty() && !subscriptions.record.contains_key(&rowid) {
              return;
            }
          }

          let Some(record_values) = extract_record_values(case) else {
            error!("Failed to extract values");
            return;
          };

          let s = ContinuationState {
            state: state.clone(),
            table_name: qualified_table_name,
            action,
            rowid,
            record_values,
          };

          // TODO: Optimization: in cases where there's only table-level access restrictions, we
          // could avoid the continuation and even dispatch the subscription handling to a
          // different thread entirely to take more work off the SQLite thread.
          conn.call_and_forget(move |conn| {
            hook_continuation(conn, s);
          });
        },
      ))
      .expect("owned conn");
  }

  async fn add_record_subscription(
    self: Arc<Self>,
    api: RecordApi,
    record: trailbase_sqlite::Value,
    user: Option<User>,
    sender: async_channel::Sender<Arc<EventPayload>>,
  ) -> Result<SubscriptionId, RecordError> {
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
    let subscription_id = SUBSCRIPTION_COUNTER.fetch_add(1, Ordering::SeqCst);
    let install_hook: bool = {
      let mut lock = self.subscriptions.write();
      let empty = lock.is_empty();
      let sender = sender.clone();

      let subscriptions = lock.entry(qualified_name.clone()).or_default();
      subscriptions
        .write()
        .record
        .entry(row_id)
        .or_default()
        .push(Subscription {
          subscription_id,
          record_api_name: api.api_name().to_string(),
          user,
          sender,
          filter: Filter::Passthrough,
        });

      empty
    };

    if install_hook {
      self.add_hook(api.clone());
    }

    return Ok(SubscriptionId {
      table_name: qualified_name.clone(),
      row_id: Some(row_id),
      sub_id: subscription_id,
    });
  }

  async fn add_table_subscription(
    self: Arc<Self>,
    api: RecordApi,
    user: Option<User>,
    filter: Option<ValueOrComposite>,
    sender: async_channel::Sender<Arc<EventPayload>>,
  ) -> Result<SubscriptionId, RecordError> {
    let filter = if let Some(filter) = filter {
      Filter::Record(qs_filter_to_record_filter(api.columns(), filter)?)
    } else {
      Filter::Passthrough
    };

    let qualified_name = api.qualified_name();
    let subscription_id = SUBSCRIPTION_COUNTER.fetch_add(1, Ordering::SeqCst);
    let install_hook: bool = {
      let mut lock = self.subscriptions.write();
      let empty = lock.is_empty();
      let sender = sender.clone();

      let subscriptions = lock.entry(qualified_name.clone()).or_default();
      subscriptions.write().table.push(Subscription {
        subscription_id,
        record_api_name: api.api_name().to_string(),
        user,
        sender,
        filter,
      });

      empty
    };

    if install_hook {
      self.add_hook(api.clone());
    }

    return Ok(SubscriptionId {
      table_name: qualified_name.clone(),
      row_id: None,
      sub_id: subscription_id,
    });
  }
}

impl Drop for PerConnectionState {
  fn drop(&mut self) {
    if let Some(first) = self.record_apis.read().values().nth(0) {
      first.conn().call_and_forget(|conn| {
        conn.preupdate_hook(NO_HOOK).expect("owned conn");
      });
    }
  }
}

/// Internal, shareable state of the cloneable SubscriptionManager.
struct ManagerState {
  /// Record API configurations.
  record_apis: Reactive<Vec<RecordApi>>,

  /// Manages subscriptions for different connections based on `conn.id()`.
  connections: RwLock<HashMap</* conn id= */ usize, Arc<PerConnectionState>>>,
}

#[derive(Clone)]
pub struct SubscriptionManager {
  state: Arc<ManagerState>,
}

struct ContinuationState {
  state: Arc<PerConnectionState>,
  table_name: QualifiedName,
  action: RecordAction,
  rowid: i64,
  record_values: Vec<rusqlite::types::Value>,
}

fn filter_record_apis(conn_id: usize, record_apis: &[RecordApi]) -> HashMap<String, RecordApi> {
  return record_apis
    .iter()
    .flat_map(|api| {
      if !api.enable_subscriptions() {
        return None;
      }
      if api.conn().id() == conn_id {
        return Some((api.api_name().to_string(), api.clone()));
      }

      return None;
    })
    .collect();
}

impl SubscriptionManager {
  pub fn new(record_apis: Reactive<HashMap<String, RecordApi>>) -> Self {
    let record_apis = derive_unchecked(&record_apis, |apis| apis.values().cloned().collect());
    let state = Arc::new(ManagerState {
      record_apis: record_apis.clone(),
      connections: RwLock::new(HashMap::new()),
    });

    {
      let state = state.clone();
      record_apis.add_observer(move |record_apis| {
        let mut lock = state.connections.write();

        let mut old: HashMap<usize, Arc<PerConnectionState>> = std::mem::take(&mut lock);

        for api in record_apis.iter() {
          if !api.enable_subscriptions() {
            continue;
          }

          let id = api.conn().id();

          // TODO: Clean subscriptions from existing entries for tables that not longer have a
          // corresponding API.
          if let Some(existing) = old.remove(&id) {
            let apis = filter_record_apis(id, record_apis);
            let Some(first) = apis.values().nth(0) else {
              continue;
            };

            // Update metadata and add back.
            *existing.connection_metadata.write() = first.connection_metadata().clone();
            *existing.record_apis.write() = apis;
            lock.insert(id, existing);
          }
        }
      });
    }

    return Self { state };
  }

  pub async fn add_sse_table_subscription(
    &self,
    api: RecordApi,
    user: Option<User>,
    filter: Option<ValueOrComposite>,
  ) -> Result<AutoCleanupEventStream, RecordError> {
    let (sender, receiver) = async_channel::bounded::<Arc<EventPayload>>(16);
    let state = self.get_per_connection_state(&api);

    let id = state
      .clone()
      .add_table_subscription(api, user, filter, sender.clone())
      .await?;

    // Send an immediate comment to flush SSE headers and establish the connection
    if sender.send(ESTABLISHED_EVENT.clone()).await.is_err() {
      return Err(RecordError::BadRequest("channel already closed"));
    }

    let receiver = AutoCleanupEventStream {
      state: AutoCleanupEventStreamState {
        receiver: receiver.downgrade(),
        state,
        id,
      },
      receiver,
    };

    return Ok(receiver);
  }

  pub async fn add_sse_record_subscription(
    &self,
    api: RecordApi,
    record: trailbase_sqlite::Value,
    user: Option<User>,
  ) -> Result<AutoCleanupEventStream, RecordError> {
    let (sender, receiver) = async_channel::bounded::<Arc<EventPayload>>(16);
    let state = self.get_per_connection_state(&api);

    let id = state
      .clone()
      .add_record_subscription(api, record, user, sender.clone())
      .await?;

    // Send an immediate comment to flush SSE headers and establish the connection
    if sender.send(ESTABLISHED_EVENT.clone()).await.is_err() {
      return Err(RecordError::BadRequest("channel already closed"));
    }

    let receiver = AutoCleanupEventStream {
      state: AutoCleanupEventStreamState {
        receiver: receiver.downgrade(),
        state,
        id,
      },
      receiver,
    };

    return Ok(receiver);
  }

  fn get_per_connection_state(&self, api: &RecordApi) -> Arc<PerConnectionState> {
    let id: usize = api.conn().id();
    let mut lock = self.state.connections.upgradable_read();
    if let Some(state) = lock.get(&id) {
      return state.clone();
    }

    return lock.with_upgraded(|m| {
      return match m.entry(id) {
        Entry::Occupied(v) => v.get().clone(),
        Entry::Vacant(v) => {
          let state = Arc::new(PerConnectionState {
            connection_metadata: RwLock::new(api.connection_metadata().clone()),
            record_apis: RwLock::new(filter_record_apis(id, &self.state.record_apis.value())),
            subscriptions: Default::default(),
          });
          v.insert(state).clone()
        }
      };
    });
  }

  #[cfg(test)]
  pub fn num_record_subscriptions(&self) -> usize {
    let mut count: usize = 0;
    for state in self.state.connections.read().values() {
      for (_table_name, subs) in state.subscriptions.read().iter() {
        for record in subs.read().record.values() {
          count += record.len();
        }
      }
    }
    return count;
  }

  #[cfg(test)]
  pub fn num_table_subscriptions(&self) -> usize {
    let mut count: usize = 0;
    for state in self.state.connections.read().values() {
      for (_table_name, subs) in state.subscriptions.read().iter() {
        count += subs.read().table.len();
      }
    }
    return count;
  }
}

fn broker_subscriptions(
  s: &PerConnectionState,
  conn: &rusqlite::Connection,
  subs: &[Subscription],
  record_subscriptions: bool,
  record: &indexmap::IndexMap<&str, rusqlite::types::Value>,
  event: &Arc<EventPayload>,
) -> Vec<usize> {
  let mut dead_subscriptions: Vec<usize> = vec![];
  for (idx, sub) in subs.iter().enumerate() {
    // Skip events for records that are being filtered out anyway.
    if let Filter::Record(ref filter) = sub.filter
      && !apply_filter_recursively_to_record(filter, record)
    {
      continue;
    }

    // We don't memoize and look up the APIs to make sure we get an up-to-date version.
    let Some(api) = s.lookup_record_api(&sub.record_api_name) else {
      dead_subscriptions.push(idx);
      sub.sender.close();
      continue;
    };

    if let Err(_err) = api.check_record_level_read_access_for_subscriptions(
      conn,
      SubscriptionAclParams {
        params: record,
        user: sub.user.as_ref(),
      },
    ) {
      // NOTE: that access failures for table subscriptions for specific records are simply ignored,
      // i.e. those events will just not be send. Other records in the table may pass the
      // check. For record subscriptions, however, missing access is a death sentence.
      if record_subscriptions {
        // This can happen if the record api configuration has changed since originally
        // subscribed. In this case we just send and error and cancel the subscription.
        match sub.sender.try_send(ACCESS_DENIED_EVENT.clone()) {
          Ok(_) | Err(TrySendError::Full(_)) => {
            sub.sender.close();
          }
          Err(TrySendError::Closed(_)) => {}
        };

        dead_subscriptions.push(idx);
      }
      continue;
    }

    // Cloning the event. It's important that we use a try_send here to not block other
    // subscriptions if a subscriber is slow and their channel fills up.
    if let Err(err) = sub.sender.try_send(event.clone()) {
      match err {
        async_channel::TrySendError::Full(ev) => {
          debug!("Channel full, dropping event: {ev:?}");
        }
        async_channel::TrySendError::Closed(_ev) => {
          dead_subscriptions.push(idx);
          sub.sender.close();
        }
      }
    }
  }

  return dead_subscriptions;
}

/// Continuation of the pre-update hook being scheduled on the Sqlite executor.
fn hook_continuation(conn: &rusqlite::Connection, s: ContinuationState) {
  let ContinuationState {
    state,
    table_name,
    action,
    rowid,
    record_values,
  } = s;

  // If table_metadata is missing, the config/schema must have changed, thus removing the
  // subscriptions.
  let lock = state.connection_metadata.read();
  let Some(table_metadata) = lock.get_table(&table_name) else {
    warn!("Table {table_name:?} not found. Removing subscriptions");

    let mut subscriptions = state.subscriptions.write();
    subscriptions.remove(&table_name);
    if subscriptions.is_empty() {
      conn.preupdate_hook(NO_HOOK).expect("owned conn");
    }

    return;
  };

  // Join values with column names.
  let record: indexmap::IndexMap<&str, rusqlite::types::Value> = record_values
    .into_iter()
    .enumerate()
    .map(|(idx, v)| (table_metadata.schema.columns[idx].name.as_str(), v))
    .collect();

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

    // let str_value = serde_json::to_string(&json_value).unwrap_or_else(|_| "{}".to_string());

    let payload = EventPayload::from(&match action {
      RecordAction::Delete => JsonEventPayload::Delete { value: json_obj },
      RecordAction::Insert => JsonEventPayload::Insert { value: json_obj },
      RecordAction::Update => JsonEventPayload::Update { value: json_obj },
    });

    Arc::new(payload)
  };

  let mut read_lock = state.subscriptions.upgradable_read();

  let (dead_record_subscriptions, dead_table_subscriptions) = {
    let Some(subscriptions) = read_lock.get(&table_name).map(|r| r.read()) else {
      return;
    };

    // First broker record subscriptions.
    let dead_record_subscriptions = subscriptions
      .record
      .get(&rowid)
      .map(|record_subscriptions| {
        broker_subscriptions(&state, conn, record_subscriptions, true, &record, &event)
      });

    // Then broker table subscriptions.
    let dead_table_subscriptions =
      broker_subscriptions(&state, conn, &subscriptions.table, false, &record, &event);

    (dead_record_subscriptions, dead_table_subscriptions)
  };

  let cleanup_record_subscriptions = dead_record_subscriptions
    .as_ref()
    .is_some_and(|dead| !dead.is_empty() || action == RecordAction::Delete);

  // Clean up, only if necessary.
  if dead_table_subscriptions.is_empty() && !cleanup_record_subscriptions {
    return;
  }

  let remove_subscription_entry_for_table = {
    let Some(mut subscriptions) = read_lock.get(&table_name).map(|l| l.write()) else {
      return;
    };

    // Record subscription cleanup.
    if let Some(dead_record_subscriptions) = dead_record_subscriptions {
      if action == RecordAction::Delete {
        // This is unique for record subscriptions: if the record is deleted, cancel all
        // subscriptions.
        subscriptions.record.remove(&rowid);
      } else if let Some(m) = subscriptions.record.get_mut(&rowid) {
        for idx in dead_record_subscriptions.iter().rev() {
          m.swap_remove(*idx);
        }

        if m.is_empty() {
          subscriptions.record.remove(&rowid);
        }
      }
    }

    // Table subscription cleanup.
    for idx in dead_table_subscriptions.iter().rev() {
      subscriptions.table.swap_remove(*idx);
    }

    subscriptions.is_empty()
  };

  if remove_subscription_entry_for_table {
    // NOTE: Only write lock across all tables when necessary.
    read_lock.with_upgraded(|lock| {
      // Check again to avoid races:
      if lock.get(&table_name).is_some_and(|e| e.read().is_empty()) {
        lock.remove(&table_name);

        if lock.is_empty() {
          conn.preupdate_hook(NO_HOOK).expect("owned conn");
        }
      }
    });
  }
}

static SUBSCRIPTION_COUNTER: AtomicI64 = AtomicI64::new(0);

static ESTABLISHED_EVENT: LazyLock<Arc<EventPayload>> =
  LazyLock::new(|| Arc::new(EventPayload::from(&JsonEventPayload::Ping)));
static ACCESS_DENIED_EVENT: LazyLock<Arc<EventPayload>> = LazyLock::new(|| {
  Arc::new(EventPayload::from(&JsonEventPayload::Error {
    error: "Access denied".into(),
  }))
});

const NO_HOOK: Option<fn(Action, &str, &str, &PreUpdateCase)> = None;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn static_sse_event_test() {
    let _x: Arc<EventPayload> = (*ACCESS_DENIED_EVENT).clone();
    let _y: Arc<EventPayload> = (*ESTABLISHED_EVENT).clone();
  }
}
