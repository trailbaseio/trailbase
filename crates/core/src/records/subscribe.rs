use async_channel::WeakReceiver;
use axum::{
  extract::{Path, RawQuery, State},
  response::sse::{Event as SseEvent, KeepAlive, Sse},
};
use futures_util::Stream;
use log::*;
use parking_lot::RwLock;
use pin_project_lite::pin_project;
use reactivate::Reactive;
use rusqlite::hooks::{Action, PreUpdateCase};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, hash_map::Entry};
use std::pin::Pin;
use std::sync::{
  Arc,
  atomic::{AtomicI64, Ordering},
};
use std::task::{Context, Poll};
use trailbase_qs::FilterQuery;
use trailbase_schema::QualifiedName;
use trailbase_schema::json::value_to_flat_json;
use trailbase_sqlite::connection::{extract_record_values, extract_row_id};

use crate::app_state::{AppState, derive_unchecked};
use crate::auth::user::User;
use crate::records::RecordApi;
use crate::records::filter::{
  Filter, apply_filter_recursively_to_record, qs_filter_to_record_filter,
};
use crate::records::record_api::SubscriptionAclParams;
use crate::records::{Permission, RecordError};
use crate::schema_metadata::ConnectionMetadata;

static SUBSCRIPTION_COUNTER: AtomicI64 = AtomicI64::new(0);

type SseEventResult = Result<SseEvent, axum::Error>;

/// Composite id uniquely identifying a subscription.
///
/// If row_id is Some, this is considered to reference a subscription to a specific record.
#[derive(Default, Debug)]
struct SubscriptionId {
  table_name: QualifiedName,
  row_id: Option<i64>,
  sub_id: i64,
}

/// RAII type for automatically cleaning up subscriptions when the receiving side gets dropped,
/// e.g. client disconnects.
struct AutoCleanupEventStreamState {
  receiver: WeakReceiver<SseEvent>,
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
  struct AutoCleanupEventStream {
    state: AutoCleanupEventStreamState,

    #[pin]
    receiver: async_channel::Receiver<SseEvent>,
  }
}

impl Stream for AutoCleanupEventStream {
  type Item = SseEventResult;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let mut this = self.project();
    let res = futures_util::ready!(this.receiver.as_mut().poll_next(cx));
    Poll::Ready(res.map(Ok))
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

impl From<Action> for RecordAction {
  fn from(value: Action) -> Self {
    return match value {
      Action::SQLITE_DELETE => RecordAction::Delete,
      Action::SQLITE_INSERT => RecordAction::Insert,
      Action::SQLITE_UPDATE => RecordAction::Update,
      _ => unreachable!("{value:?}"),
    };
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DbEvent {
  Update(Option<serde_json::Value>),
  Insert(Option<serde_json::Value>),
  Delete(Option<serde_json::Value>),
  Error(String),
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
  sender: async_channel::Sender<SseEvent>,
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
      // NOTE: Only write lock across all tables when necessary.
      read_lock.with_upgraded(|lock| {
        // Check again to avoid races:
        if lock
          .get(&id.table_name)
          .is_some_and(|e| e.read().is_empty())
        {
          // Check again.
          lock.remove(&id.table_name);

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
          let action: RecordAction = match action {
            Action::SQLITE_UPDATE | Action::SQLITE_INSERT | Action::SQLITE_DELETE => action.into(),
            a => {
              error!("Unknown action: {a:?}");
              return;
            }
          };

          let Some(rowid) = extract_row_id(case) else {
            error!("Failed to extract row id");
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
  ) -> Result<AutoCleanupEventStream, RecordError> {
    let table_name = api.table_name();
    let qualified_table_name = api.qualified_name().clone();

    let pk_column = &api.record_pk_column().1.name;

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

    let (sender, receiver) = async_channel::bounded::<SseEvent>(16);

    let subscription_id = SUBSCRIPTION_COUNTER.fetch_add(1, Ordering::SeqCst);
    let install_hook: bool = {
      let mut lock = self.subscriptions.write();
      let empty = lock.is_empty();

      let subscriptions = lock.entry(api.qualified_name().clone()).or_default();
      subscriptions
        .write()
        .record
        .entry(row_id)
        .or_default()
        .push(Subscription {
          subscription_id,
          record_api_name: api.api_name().to_string(),
          user,
          sender: sender.clone(),
          filter: Filter::Passthrough,
        });

      empty
    };

    if install_hook {
      self.add_hook(api);
    }

    // Send an immediate comment to flush SSE headers and establish the connection
    let _ = sender
      .send(SseEvent::default().comment("subscription established"))
      .await;

    return Ok(AutoCleanupEventStream {
      state: AutoCleanupEventStreamState {
        receiver: receiver.downgrade(),
        state: self,
        id: SubscriptionId {
          table_name: qualified_table_name,
          row_id: Some(row_id),
          sub_id: subscription_id,
        },
      },
      receiver,
    });
  }

  async fn add_table_subscription(
    self: Arc<Self>,
    api: RecordApi,
    user: Option<User>,
    filter: Option<trailbase_qs::ValueOrComposite>,
  ) -> Result<AutoCleanupEventStream, RecordError> {
    let table_name = api.qualified_name().clone();

    let filter = if let Some(filter) = filter {
      Filter::Record(qs_filter_to_record_filter(api.columns(), filter)?)
    } else {
      Filter::Passthrough
    };

    let (sender, receiver) = async_channel::bounded::<SseEvent>(16);
    let subscription_id = SUBSCRIPTION_COUNTER.fetch_add(1, Ordering::SeqCst);
    let install_hook: bool = {
      let mut lock = self.subscriptions.write();
      let empty = lock.is_empty();

      let subscriptions = lock.entry(api.qualified_name().clone()).or_default();
      subscriptions.write().table.push(Subscription {
        subscription_id,
        record_api_name: api.api_name().to_string(),
        user,
        sender: sender.clone(),
        filter,
      });

      empty
    };

    if install_hook {
      self.add_hook(api);
    }

    // Send an immediate comment to flush SSE headers and establish the connection
    let _ = sender
      .send(SseEvent::default().comment("subscription established"))
      .await;

    return Ok(AutoCleanupEventStream {
      state: AutoCleanupEventStreamState {
        receiver: receiver.downgrade(),
        state: self,
        id: SubscriptionId {
          table_name,
          row_id: None,
          sub_id: subscription_id,
        },
      },
      receiver,
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

  async fn add_table_subscription(
    &self,
    api: RecordApi,
    user: Option<User>,
    filter: Option<trailbase_qs::ValueOrComposite>,
  ) -> Result<AutoCleanupEventStream, RecordError> {
    return self
      .get_per_connection_state(&api)
      .add_table_subscription(api, user, filter)
      .await;
  }

  async fn add_record_subscription(
    &self,
    api: RecordApi,
    record: trailbase_sqlite::Value,
    user: Option<User>,
  ) -> Result<AutoCleanupEventStream, RecordError> {
    return self
      .get_per_connection_state(&api)
      .add_record_subscription(api, record, user)
      .await;
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
  event: &SseEvent,
) -> Vec<usize> {
  let mut dead_subscriptions: Vec<usize> = vec![];
  for (idx, sub) in subs.iter().enumerate() {
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
      if record_subscriptions {
        // This can happen if the record api configuration has changed since originally
        // subscribed. In this case we just send and error and cancel the subscription.
        if let Ok(ev) = SseEvent::default().json_data(DbEvent::Error("Access denied".into())) {
          let _ = sub.sender.try_send(ev);
        }

        dead_subscriptions.push(idx);
        sub.sender.close();
      }
      continue;
    }

    if let Filter::Record(ref filter) = sub.filter
      && !apply_filter_recursively_to_record(filter, record)
    {
      continue;
    }

    // Cloning the event.
    if let Err(err) = sub.sender.try_send(event.clone()) {
      match err {
        async_channel::TrySendError::Full(ev) => {
          warn!("Channel full, dropping event: {ev:?}");
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

/// Continuation of the pre-update hook being scheduled on the executor.
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
  let event = {
    let json_value = serde_json::Value::Object(
      record
        .iter()
        .filter_map(|(name, value)| {
          if let Ok(v) = value_to_flat_json(value) {
            return Some(((*name).to_string(), v));
          };
          return None;
        })
        .collect(),
    );

    let db_event = match action {
      RecordAction::Delete => DbEvent::Delete(Some(json_value)),
      RecordAction::Insert => DbEvent::Insert(Some(json_value)),
      RecordAction::Update => DbEvent::Update(Some(json_value)),
    };

    let Ok(event) = SseEvent::default().json_data(db_event) else {
      return;
    };

    event
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

  // .Clean up if necessary
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

/// Read record.
#[utoipa::path(
  get,
  path = "/{name}/subscribe/{record}",
  tag = "records",
  responses(
    (status = 200, description = "SSE stream of record changes.")
  )
)]
pub async fn add_subscription_sse_handler(
  State(state): State<AppState>,
  Path((api_name, record)): Path<(String, String)>,
  user: Option<User>,
  RawQuery(raw_url_query): RawQuery,
) -> Result<Sse<impl Stream<Item = SseEventResult>>, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  if !api.enable_subscriptions() {
    return Err(RecordError::Forbidden);
  }

  let FilterQuery { filter } = raw_url_query
    .as_ref()
    .map_or_else(
      || Ok(FilterQuery::default()),
      |query| FilterQuery::parse(query),
    )
    .map_err(|_err| {
      return RecordError::BadRequest("Invalid query");
    })?;

  return match record.as_str() {
    "*" => {
      api.check_table_level_access(Permission::Read, user.as_ref())?;

      let receiver = state
        .subscription_manager()
        .add_table_subscription(api, user, filter)
        .await?;

      Ok(Sse::new(receiver).keep_alive(KeepAlive::default()))
    }
    _ => {
      let record_id = api.primary_key_to_value(record)?;
      api
        .check_record_level_access(Permission::Read, Some(&record_id), None, user.as_ref())
        .await?;

      let receiver = state
        .subscription_manager()
        .add_record_subscription(api, record_id, user)
        .await?;

      Ok(Sse::new(receiver).keep_alive(KeepAlive::default()))
    }
  };
}

#[cfg(test)]
mod tests {
  use async_channel::TryRecvError;
  use axum::response::IntoResponse;
  use futures_util::StreamExt;
  use trailbase_sqlite::params;

  use super::DbEvent;
  use super::*;

  use crate::admin::user::*;
  use crate::app_state::test_state;
  use crate::auth::util::login_with_password;
  use crate::config::proto::RecordApiConfig;
  use crate::records::PermissionFlag;
  use crate::records::test_utils::add_record_api_config;
  use crate::util::uuid_to_b64;

  async fn decode_db_event(event: SseEvent) -> Option<DbEvent> {
    let (sender, receiver) = async_channel::unbounded::<SseEvent>();
    let sse = Sse::new(receiver.map(|ev| -> Result<SseEvent, axum::Error> { Ok(ev) }));

    sender.send(event.clone()).await.unwrap();
    sender.close();

    let resp = sse.into_response();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
      .await
      .unwrap();

    let str = String::from_utf8_lossy(&bytes);
    let Some(data) = str.strip_prefix("data: ") else {
      // There are non-data events such as "subscription established" or heartbeat.
      return None;
    };

    let x = data.strip_suffix("\n\n").unwrap();
    return serde_json::from_str(x).unwrap();
  }

  #[tokio::test]
  async fn sse_event_encoding_test() {
    let json = serde_json::json!({
      "a": 5,
      "b": "text",
    });
    let db_event = DbEvent::Delete(Some(json));
    let event = SseEvent::default().json_data(db_event.clone()).unwrap();

    assert_eq!(decode_db_event(event).await.unwrap(), db_event);
  }

  async fn setup_world_readable() -> AppState {
    let state = test_state(None).await.unwrap();
    let conn = state.conn().clone();

    conn
      .execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, text TEXT) STRICT",
        (),
      )
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    // Register message table as record api with moderator read access.
    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("api_name".to_string()),
        table_name: Some("test".to_string()),
        enable_subscriptions: Some(true),
        acl_world: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    return state;
  }

  #[tokio::test]
  async fn subscribe_to_record_test() {
    let state = setup_world_readable().await;
    let conn = state.conn().clone();

    let record_id_raw = 0;
    let record_id = trailbase_sqlite::Value::Integer(record_id_raw);
    let rowid: i64 = conn
      .query_row_f(
        "INSERT INTO test (id, text) VALUES ($1, 'foo') RETURNING _rowid_",
        [record_id],
        |row| row.get(0),
      )
      .await
      .unwrap()
      .unwrap();

    assert_eq!(rowid, record_id_raw);

    let manager = state.subscription_manager();
    let api = state.lookup_record_api("api_name").unwrap();
    let stream = manager
      .add_record_subscription(api, trailbase_sqlite::Value::Integer(0), None)
      .await
      .unwrap();

    assert_eq!(1, manager.num_record_subscriptions());

    // Make sure rebuilding connection metadata doesn't drop subscriptions.
    state.rebuild_connection_metadata().await.unwrap();

    assert_eq!(1, manager.num_record_subscriptions());

    // Make sure updating config doesn't drop subscriptions.
    state
      .validate_and_update_config(state.get_config(), None)
      .await
      .unwrap();

    // First event is "connection established".
    assert!(
      decode_db_event(stream.receiver.recv().await.unwrap())
        .await
        .is_none(),
    );

    // This should do nothing since nobody is subscribed to id = 5.
    let _ = conn
      .execute(
        "INSERT INTO test (id, text) VALUES ($1, 'baz')",
        [trailbase_sqlite::Value::Integer(5)],
      )
      .await
      .unwrap();

    conn
      .execute(
        "UPDATE test SET text = $1 WHERE _rowid_ = $2",
        params!("bar", rowid),
      )
      .await
      .unwrap();

    let expected = serde_json::json!({
      "id": record_id_raw,
      "text": "bar",
    });
    match decode_db_event(stream.receiver.recv().await.unwrap()).await {
      Some(DbEvent::Update(Some(value))) => {
        assert_eq!(value, expected);
      }
      x => {
        panic!("Expected update, got: {x:?}");
      }
    };

    conn
      .execute("DELETE FROM test WHERE _rowid_ = $1", params!(rowid))
      .await
      .unwrap();

    match decode_db_event(stream.receiver.recv().await.unwrap()).await {
      Some(DbEvent::Delete(Some(value))) => {
        assert_eq!(value, expected);
      }
      x => {
        panic!("Expected delete, got: {x:?}");
      }
    }

    // Implicitly await for scheduled cleanups to go through.
    conn
      .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
      .await
      .unwrap();

    assert_eq!(0, manager.num_record_subscriptions());
  }

  #[tokio::test]
  async fn subscribe_to_table_test() {
    let state = setup_world_readable().await;
    let conn = state.conn().clone();

    let manager = state.subscription_manager();
    let api = state.lookup_record_api("api_name").unwrap();

    {
      let stream = manager
        .add_table_subscription(api, None, None)
        .await
        .unwrap();

      assert_eq!(1, manager.num_table_subscriptions());
      // First event is "connection established".
      assert!(
        decode_db_event(stream.receiver.recv().await.unwrap())
          .await
          .is_none(),
      );

      let record_id_raw = 0;
      conn
        .execute(
          "INSERT INTO test (id, text) VALUES ($1, 'foo')",
          params!(record_id_raw),
        )
        .await
        .unwrap();

      conn
        .execute(
          "UPDATE test SET text = $1 WHERE id = $2",
          params!("bar", record_id_raw),
        )
        .await
        .unwrap();

      match decode_db_event(stream.receiver.recv().await.unwrap()).await {
        Some(DbEvent::Insert(Some(value))) => {
          let expected = serde_json::json!({
            "id": record_id_raw,
            "text": "foo",
          });
          assert_eq!(value, expected);
        }
        x => {
          panic!("Expected insert, got: {x:?}");
        }
      };

      let expected = serde_json::json!({
        "id": record_id_raw,
        "text": "bar",
      });
      match decode_db_event(stream.receiver.recv().await.unwrap()).await {
        Some(DbEvent::Update(Some(value))) => {
          assert_eq!(value, expected);
        }
        x => {
          panic!("Expected update, got: {x:?}");
        }
      };

      conn
        .execute("DELETE FROM test WHERE id = $1", params!(record_id_raw))
        .await
        .unwrap();

      match decode_db_event(stream.receiver.recv().await.unwrap()).await {
        Some(DbEvent::Delete(Some(value))) => {
          assert_eq!(value, expected);
        }
        x => {
          panic!("Expected delete, got: {x:?}");
        }
      }
    }

    // Implicitly await for scheduled cleanups to go through.
    conn
      .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
      .await
      .unwrap();

    assert_eq!(0, manager.num_table_subscriptions());
  }

  #[tokio::test]
  async fn subscription_lifecycle_test() {
    let state = setup_world_readable().await;
    let conn = state.conn().clone();

    let record_id_raw = 0;
    let record_id = trailbase_sqlite::Value::Integer(record_id_raw);
    let rowid: i64 = conn
      .query_row_f(
        "INSERT INTO test (id, text) VALUES ($1, 'foo') RETURNING _rowid_",
        [record_id],
        |row| row.get(0),
      )
      .await
      .unwrap()
      .unwrap();

    assert_eq!(rowid, record_id_raw);

    let sse = add_subscription_sse_handler(
      State(state.clone()),
      Path(("api_name".to_string(), record_id_raw.to_string())),
      None,
      RawQuery(None),
    )
    .await;

    let manager = state.subscription_manager();
    assert_eq!(1, manager.num_record_subscriptions());

    drop(sse);

    // Implicitly await for the cleanup to be scheduled on the sqlite executor.
    conn
      .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
      .await
      .unwrap();

    assert_eq!(0, manager.num_record_subscriptions());
  }

  async fn setup_with_tight_acls() -> AppState {
    let state = test_state(None).await.unwrap();
    let conn = state.conn().clone();

    conn
      .execute(
        "CREATE TABLE test (
            id          INTEGER PRIMARY KEY,
            user        BLOB NOT NULL,
            text        TEXT
         ) STRICT",
        (),
      )
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    // Register message table as record api with moderator read access.
    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("api_name".to_string()),
        table_name: Some("test".to_string()),
        enable_subscriptions: Some(true),
        acl_authenticated: [PermissionFlag::Read as i32].into(),
        read_access_rule: Some(
          "EXISTS(SELECT 1 FROM test AS m WHERE _USER_.id = _ROW_.user)".to_string(),
        ),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    return state;
  }

  #[tokio::test]
  async fn subscription_acl_test() {
    let state = setup_with_tight_acls().await;
    let conn = state.conn();

    let user_x_email = "user_x@bar.com";
    let password = "Secret!1!!";

    let sse_or = add_subscription_sse_handler(
      State(state.clone()),
      Path(("api_name".to_string(), "*".to_string())),
      None,
      RawQuery(None),
    )
    .await;

    assert!(matches!(sse_or, Err(RecordError::Forbidden)));

    let user_x = create_user_for_test(&state, user_x_email, password)
      .await
      .unwrap()
      .into_bytes();
    let user_x_token = login_with_password(&state, user_x_email, password)
      .await
      .unwrap();

    // Check that we can subscribe to table wide changes.
    {
      let _ = add_subscription_sse_handler(
        State(state.clone()),
        Path(("api_name".to_string(), "*".to_string())),
        User::from_auth_token(&state, &user_x_token.auth_token),
        RawQuery(None),
      )
      .await
      .unwrap();
    }

    let record_id_raw = 0;
    let record_id = trailbase_sqlite::Value::Integer(record_id_raw);
    let _rowid: i64 = conn
      .query_row_f(
        "INSERT INTO test (id, user, text) VALUES ($1, $2, 'foo') RETURNING _rowid_",
        [
          record_id.clone(),
          trailbase_sqlite::Value::Blob(user_x.to_vec()),
        ],
        |row| row.get(0),
      )
      .await
      .unwrap()
      .unwrap();

    // Assert user_x can subscribe to their record.
    {
      let _ = add_subscription_sse_handler(
        State(state.clone()),
        Path(("api_name".to_string(), record_id_raw.to_string())),
        User::from_auth_token(&state, &user_x_token.auth_token),
        RawQuery(None),
      )
      .await
      .unwrap();
    }

    // Assert user_y cannot subscribe to user_x's record.
    {
      let user_y_email = "user_y@bar.com";
      let _user_y = create_user_for_test(&state, user_y_email, password)
        .await
        .unwrap()
        .into_bytes();
      let user_y_token = login_with_password(&state, user_y_email, password)
        .await
        .unwrap();

      let sse_or = add_subscription_sse_handler(
        State(state.clone()),
        Path(("api_name".to_string(), record_id_raw.to_string())),
        User::from_auth_token(&state, &user_y_token.auth_token),
        RawQuery(None),
      )
      .await;

      assert!(matches!(sse_or, Err(RecordError::Forbidden)));
    }
  }

  #[tokio::test]
  async fn test_acl_selective_table_subs() {
    let state = setup_with_tight_acls().await;
    let conn = state.conn();

    let manager = state.subscription_manager();
    let api = state.lookup_record_api("api_name").unwrap();

    let password = "Secret!1!!";
    let user_x_email = "user_x@bar.com";
    let user_x = create_user_for_test(&state, user_x_email, password)
      .await
      .unwrap();
    let user_x_token = login_with_password(&state, user_x_email, password)
      .await
      .unwrap();

    let user_y_email = "user_y@bar.com";
    let _user_y = create_user_for_test(&state, user_y_email, password)
      .await
      .unwrap()
      .into_bytes();
    let user_y_token = login_with_password(&state, user_y_email, password)
      .await
      .unwrap();

    // Assert events for table subscriptions are selective on ACLs.
    {
      let user_x_subscription = manager
        .add_table_subscription(
          api.clone(),
          User::from_auth_token(&state, &user_x_token.auth_token),
          None,
        )
        .await
        .unwrap();

      // First event is "connection established".
      assert!(
        decode_db_event(user_x_subscription.receiver.recv().await.unwrap())
          .await
          .is_none(),
      );

      let user_y_subscription = manager
        .add_table_subscription(
          api.clone(),
          User::from_auth_token(&state, &user_y_token.auth_token),
          None,
        )
        .await
        .unwrap();

      assert_eq!(2, manager.num_table_subscriptions());

      let record_id_raw = 1;
      conn
        .execute(
          "INSERT INTO test (id, user, text) VALUES ($1, $2, 'foo')",
          [
            trailbase_sqlite::Value::Integer(record_id_raw),
            trailbase_sqlite::Value::Blob(user_x.into()),
          ],
        )
        .await
        .unwrap();

      match decode_db_event(user_x_subscription.receiver.recv().await.unwrap()).await {
        Some(DbEvent::Insert(Some(value))) => {
          let expected = serde_json::json!({
            "id": record_id_raw,
            "user": uuid_to_b64(&user_x),
            "text": "foo",
          });
          assert_eq!(value, expected);
        }
        x => {
          panic!("Expected insert, got: {x:?}");
        }
      };

      // User y should *not* have received the insert event.
      assert!(
        tokio::time::timeout(
          tokio::time::Duration::from_millis(300),
          user_y_subscription.receiver.clone().count()
        )
        .await
        .is_err()
      );
      assert_eq!(
        user_y_subscription.receiver.try_recv().err().unwrap(),
        TryRecvError::Empty
      );
    }

    // Implicitly await for scheduled cleanups to go through.
    conn
      .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
      .await
      .unwrap();

    assert_eq!(0, manager.num_table_subscriptions());
  }

  #[tokio::test]
  async fn subscription_acl_change_owner() {
    let state = setup_with_tight_acls().await;
    let conn = state.conn();

    let user_x_email = "user_x@bar.com";
    let password = "Secret!1!!";

    let user_x_id = create_user_for_test(&state, user_x_email, password)
      .await
      .unwrap();
    let user_x_token = login_with_password(&state, user_x_email, password)
      .await
      .unwrap();
    let user_x = User::from_auth_token(&state, &user_x_token.auth_token);

    let record_id = 0;
    let _rowid: i64 = conn
      .query_row_f(
        "INSERT INTO test (id, user, text) VALUES ($1, $2, 'foo') RETURNING _rowid_",
        [
          trailbase_sqlite::Value::Integer(record_id),
          trailbase_sqlite::Value::Blob(user_x_id.into()),
        ],
        |row| row.get(0),
      )
      .await
      .unwrap()
      .unwrap();

    let manager = state.subscription_manager();
    let api = state.lookup_record_api("api_name").unwrap();
    let stream = manager
      .add_record_subscription(api, trailbase_sqlite::Value::Integer(record_id), user_x)
      .await
      .unwrap();

    assert_eq!(1, manager.num_record_subscriptions());
    // First event is "connection established".
    assert!(
      decode_db_event(stream.receiver.recv().await.unwrap())
        .await
        .is_none(),
    );

    conn
      .execute(
        "UPDATE test SET text = $1 WHERE id = $2",
        params!("bar", record_id),
      )
      .await
      .unwrap();

    // Unset the owner. This Update should no longer be delivered.
    conn
      .execute(
        "UPDATE test SET user = $1 WHERE id = $2",
        params!(Vec::<u8>::new(), record_id),
      )
      .await
      .unwrap();

    match decode_db_event(stream.receiver.recv().await.unwrap()).await {
      Some(DbEvent::Update(Some(value))) => {
        let expected = serde_json::json!({
          "id": record_id,
          "user": uuid_to_b64(&user_x_id),
          "text": "bar",
        });
        assert_eq!(value, expected);
      }
      x => {
        panic!("Expected update, got: {x:?}");
      }
    }

    match decode_db_event(stream.receiver.recv().await.unwrap()).await {
      Some(DbEvent::Error(_msg)) => {}
      x => {
        panic!("Expected error, got: {x:?}");
      }
    }

    conn
      .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
      .await
      .unwrap();

    // Make sure the subscription was cleaned up after the access error.
    assert!(stream.receiver.is_closed());
    assert_eq!(0, manager.num_record_subscriptions());
  }

  #[tokio::test]
  async fn subscription_filter_test() {
    let state = setup_world_readable().await;
    let conn = state.conn().clone();

    let manager = state.subscription_manager();
    let api = state.lookup_record_api("api_name").unwrap();

    {
      let filter =
        FilterQuery::parse("filter[$and][0][id][$gt]=5&filter[$and][1][id][$lt]=100").unwrap();

      let stream = manager
        .add_table_subscription(api, None, Some(filter.filter.unwrap()))
        .await
        .unwrap();

      assert_eq!(1, manager.num_table_subscriptions());
      // First event is "connection established".
      assert!(
        decode_db_event(stream.receiver.recv().await.unwrap())
          .await
          .is_none(),
      );

      // This one should be filter out.
      conn
        .execute("INSERT INTO test (id, text) VALUES ($1, 'foo')", params!(1))
        .await
        .unwrap();

      // This one should get through.
      conn
        .execute(
          "INSERT INTO test (id, text) VALUES ($1, 'foo')",
          params!(25),
        )
        .await
        .unwrap();

      match decode_db_event(stream.receiver.recv().await.unwrap()).await {
        Some(DbEvent::Insert(Some(value))) => {
          let expected = serde_json::json!({
            "id": 25,
            "text": "foo",
          });
          assert_eq!(value, expected);
        }
        x => {
          panic!("Expected insert, got: {x:?}");
        }
      };
    }

    // Implicitly await for scheduled cleanups to go through.
    conn
      .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
      .await
      .unwrap();

    assert_eq!(0, manager.num_table_subscriptions());
  }
}

const NO_HOOK: Option<fn(Action, &str, &str, &PreUpdateCase)> = None;
