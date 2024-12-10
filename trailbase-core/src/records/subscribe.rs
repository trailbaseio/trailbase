use axum::{
  extract::{Path, State},
  response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{Stream, StreamExt};
use parking_lot::RwLock;
use rusqlite::hooks::{Action, PreUpdateCase};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{
  atomic::{AtomicI64, Ordering},
  Arc,
};
use trailbase_sqlite::{
  connection::{extract_record_values, extract_row_id},
  params,
};

use crate::auth::user::User;
use crate::records::sql_to_json::value_to_json;
use crate::records::RecordApi;
use crate::records::{Permission, RecordError};
use crate::table_metadata::{TableMetadata, TableMetadataCache};
use crate::value_notifier::Computed;
use crate::AppState;

static SUBSCRIPTION_COUNTER: AtomicI64 = AtomicI64::new(0);

// TODO:
//  * clients
//  * table-wide subscriptions
//  * optimize: avoid repeated encoding of events. Easy to do but makes testing harder since there's
//    no good way to parse sse::Event back :/. We should probably just bite the bullet and parse,
//    it's literally "data: <json>\n\n".

type SseEvent = Result<axum::response::sse::Event, axum::Error>;

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

#[derive(Debug, Clone, Serialize)]
pub enum DbEvent {
  Update(Option<serde_json::Value>),
  Insert(Option<serde_json::Value>),
  Delete(Option<serde_json::Value>),
  Error(String),
}

// pub struct SubscriptionId {
//   table_name: String,
//   row_id: i64,
//   subscription_id: i64,
// }

pub struct Subscription {
  /// Id uniquely identifying this subscription.
  subscription_id: i64,
  /// Name of the API this subscription is subscribed to. We need to lookup the Record API on the
  /// hot path to make sure we're getting the latest configuration.
  record_api_name: String,

  /// Record id present for subscriptions to specific records.
  // record_id: Option<trailbase_sqlite::Value>,
  user: Option<User>,
  /// Channel for sending events to the SSE handler.
  channel: async_channel::Sender<DbEvent>,
}

/// Internal, shareable state of the cloneable SubscriptionManager.
struct ManagerState {
  /// SQLite connection to monitor.
  conn: trailbase_sqlite::Connection,
  /// Table metadata for mapping column indexes to column names needed for building JSON encoded
  /// records.
  table_metadata: TableMetadataCache,
  /// Record API configurations.
  record_apis: Computed<Vec<(String, RecordApi)>, crate::config::proto::Config>,

  /// Map from table name to row id to list of subscriptions.
  subscriptions: RwLock<HashMap<String, HashMap<i64, Vec<Subscription>>>>,
}

impl ManagerState {
  fn lookup_record_api(&self, name: &str) -> Option<RecordApi> {
    for (record_api_name, record_api) in self.record_apis.load().iter() {
      if record_api_name == name {
        return Some(record_api.clone());
      }
    }
    return None;
  }
}

#[derive(Clone)]
pub struct SubscriptionManager {
  state: Arc<ManagerState>,
}

struct ContinuationState {
  state: Arc<ManagerState>,
  table_metadata: Arc<TableMetadata>,
  action: RecordAction,
  table_name: String,
  rowid: i64,
  record_values: Vec<rusqlite::types::Value>,
}

impl SubscriptionManager {
  pub fn new(
    conn: trailbase_sqlite::Connection,
    table_metadata: TableMetadataCache,
    record_apis: Computed<Vec<(String, RecordApi)>, crate::config::proto::Config>,
  ) -> Self {
    return Self {
      state: Arc::new(ManagerState {
        conn,
        table_metadata,
        record_apis,

        subscriptions: RwLock::new(HashMap::new()),
      }),
    };
  }

  pub fn num_subscriptions(&self) -> usize {
    let mut count: usize = 0;
    for table in self.state.subscriptions.read().values() {
      for record in table.values() {
        count += record.len();
      }
    }
    return count;
  }

  /// Preupdate hook that runs in a continuation of the trailbase-sqlite executor.
  fn hook_continuation(conn: &rusqlite::Connection, state: ContinuationState) {
    let ContinuationState {
      state,
      table_metadata,
      table_name,
      action,
      rowid,
      record_values,
    } = state;

    let s = &state;

    let mut read_lock = s.subscriptions.upgradable_read();
    let Some(subs) = read_lock.get(&table_name).and_then(|m| m.get(&rowid)) else {
      return;
    };

    // Join values with column names.
    let record: Vec<_> = record_values
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
            if let Ok(v) = value_to_json(value.clone()) {
              return Some(((*name).to_string(), v));
            };
            return None;
          })
          .collect(),
      );

      match action {
        RecordAction::Delete => DbEvent::Delete(Some(json_value)),
        RecordAction::Insert => DbEvent::Insert(Some(json_value)),
        RecordAction::Update => DbEvent::Update(Some(json_value)),
      }
    };

    let mut dead_subscriptions: Vec<usize> = vec![];
    for (idx, sub) in subs.iter().enumerate() {
      let Some(api) = s.lookup_record_api(&sub.record_api_name) else {
        dead_subscriptions.push(idx);
        continue;
      };

      if let Err(_err) = api.check_record_level_read_access(
        conn,
        Permission::Read,
        // TODO: Maybe we could inject ValueRef instead to avoid repeated cloning.
        record.clone(),
        sub.user.as_ref(),
      ) {
        // This can happen if the record api configuration has changed since originally
        // subscribed. In this case we just send and error and cancel the subscription.
        let _ = sub.channel.try_send(DbEvent::Error("Access denied".into()));
        dead_subscriptions.push(idx);
        continue;
      }

      // TODO: Avoid cloning the event/record over and over.
      match sub.channel.try_send(event.clone()) {
        Ok(_) => {}
        Err(async_channel::TrySendError::Full(ev)) => {
          log::warn!("Channel full, dropping event: {ev:?}");
        }
        Err(async_channel::TrySendError::Closed(_ev)) => {
          dead_subscriptions.push(idx);
        }
      }
    }

    if dead_subscriptions.is_empty() && action != RecordAction::Delete {
      // No cleanup needed.
      return;
    }

    read_lock.with_upgraded(move |subscriptions| {
      let Some(table_subscriptions) = subscriptions.get_mut(&table_name) else {
        return;
      };

      if action == RecordAction::Delete {
        // Also drops the channel and thus automatically closes the SSE connection.
        table_subscriptions.remove(&rowid);

        if table_subscriptions.is_empty() {
          subscriptions.remove(&table_name);
          if subscriptions.is_empty() {
            conn.preupdate_hook(NO_HOOK);
          }
        }

        return;
      }

      if let Some(m) = table_subscriptions.get_mut(&rowid) {
        for idx in dead_subscriptions.iter().rev() {
          m.swap_remove(*idx);
        }

        if m.is_empty() {
          table_subscriptions.remove(&rowid);

          if table_subscriptions.is_empty() {
            subscriptions.remove(&table_name);
            if subscriptions.is_empty() {
              conn.preupdate_hook(NO_HOOK);
            }
          }
        }
      }
    });
  }

  async fn add_hook(&self) -> trailbase_sqlite::connection::Result<()> {
    let state = &self.state;
    let conn = state.conn.clone();
    let s = state.clone();

    return state
      .conn
      .add_preupdate_hook(Some(
        move |action: Action, db: &str, table_name: &str, case: &PreUpdateCase| {
          assert_eq!(db, "main");

          let action: RecordAction = match action {
            Action::SQLITE_UPDATE | Action::SQLITE_INSERT | Action::SQLITE_DELETE => action.into(),
            a => {
              log::error!("Unknown action: {a:?}");
              return;
            }
          };

          let Some(rowid) = extract_row_id(case) else {
            log::error!("Failed to extract row id");
            return;
          };

          // If there are no subscriptions, do nothing.
          if s
            .subscriptions
            .read()
            .get(table_name)
            .and_then(|m| m.get(&rowid))
            .is_none()
          {
            return;
          }

          let Some(table_metadata) = s.table_metadata.get(table_name) else {
            // TODO: Should we cleanup here? Probably, since we won't recover from this issue.
            log::error!("Table not found: {table_name}");
            return;
          };

          let Some(record_values) = extract_record_values(case) else {
            log::error!("Failed to extract values");
            return;
          };

          let state = ContinuationState {
            state: s.clone(),
            table_metadata,
            action,
            table_name: table_name.to_string(),
            rowid,
            record_values,
          };

          // TODO: Optimization: in cases where there's only table-level access restrictions, we
          // could avoid the continuation and even dispatch the subscription handling to a
          // different thread entirely to take more work off the SQLite thread.
          conn.call_and_forget(move |conn| {
            Self::hook_continuation(conn, state);
          });
        },
      ))
      .await;
  }

  async fn add_subscription(
    &self,
    api: RecordApi,
    record: Option<trailbase_sqlite::Value>,
    user: Option<User>,
  ) -> Result<async_channel::Receiver<DbEvent>, RecordError> {
    let Some(record) = record else {
      return Err(RecordError::BadRequest("Missing record id"));
    };
    let (sender, receiver) = async_channel::bounded::<DbEvent>(16);

    let table_name = api.table_name();
    let pk_column = &api.record_pk_column().name;

    let Some(row) = self
      .state
      .conn
      .query_row(
        &format!(r#"SELECT _rowid_ FROM "{table_name}" WHERE "{pk_column}" = $1"#),
        params!(record.clone()),
      )
      .await?
    else {
      return Err(RecordError::RecordNotFound);
    };
    let row_id: i64 = row
      .get(0)
      .map_err(|err| RecordError::Internal(err.into()))?;

    let subscription_id = SUBSCRIPTION_COUNTER.fetch_add(1, Ordering::SeqCst);
    let empty = {
      let mut lock = self.state.subscriptions.write();
      let empty = lock.is_empty();
      let m: &mut HashMap<i64, Vec<Subscription>> = lock.entry(table_name.to_string()).or_default();

      m.entry(row_id).or_default().push(Subscription {
        subscription_id,
        record_api_name: api.api_name().to_string(),
        // record_id: Some(record),
        user,
        channel: sender,
      });

      empty
    };

    if empty {
      self.add_hook().await.unwrap();
    }

    return Ok(receiver);
  }

  // TODO: Cleaning up subscriptions might be a thing, e.g. if SSE handlers had an onDisconnect
  // handler. Right now we're handling cleanups reactively, i.e. we only remove subscriptions when
  // sending new events and the receiving end of a handler channel became invalid. It would
  // be better to be pro-active and remove subscriptions sooner.
  //
  // async fn cleanup_subscription(&self, subscription_id: SubscriptionId) -> Result<(),
  // RecordError> {   let mut lock = self.state.subscriptions.write();
  //
  //   if let Some(table_subs) = lock.get_mut(&subscription_id.table_name) {
  //     if let Some(subs) = table_subs.get_mut(&subscription_id.row_id) {
  //       subs.retain(|s| s.id != subscription_id.subscription_id);
  //
  //       if subs.is_empty() {
  //         table_subs.remove(&subscription_id.row_id);
  //       }
  //     }
  //
  //     if table_subs.is_empty() {
  //       lock.remove(&subscription_id.table_name);
  //     }
  //   }
  //
  //   if lock.is_empty() {
  //     Self::remove_preupdate_hook(&*self.state).await?;
  //   }
  //
  //   return Ok(());
  // }
}

pub async fn add_subscription_sse_handler(
  State(state): State<AppState>,
  Path((api_name, record)): Path<(String, String)>,
  user: Option<User>,
) -> Result<Sse<impl Stream<Item = SseEvent>>, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  let record_id = api.id_to_sql(&record)?;

  let Ok(()) = api
    .check_record_level_access(Permission::Read, Some(&record_id), None, user.as_ref())
    .await
  else {
    return Err(RecordError::Forbidden);
  };

  let receiver = state
    .subscription_manager()
    .add_subscription(api, Some(record_id), user)
    .await?;

  return Ok(
    Sse::new(receiver.map(|ev| {
      // TODO: We're re-encoding the event over and over again for all subscriptions. Would be easy
      // to pre-encode on the sender side but makes testing much harder, since there's no good way
      // to parse sse::Event back.
      return Event::default().json_data(ev);
    }))
    .keep_alive(KeepAlive::default()),
  );
}

#[cfg(test)]
mod tests {
  use super::DbEvent;
  use super::*;
  use crate::app_state::test_state;
  use crate::records::{add_record_api, AccessRules, Acls, PermissionFlag};

  #[tokio::test]
  async fn subscribe_connection_test() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn().clone();

    conn
      .execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, text TEXT) STRICT",
        (),
      )
      .await
      .unwrap();

    state.table_metadata().invalidate_all().await.unwrap();

    // Register message table as record api with moderator read access.
    add_record_api(
      &state,
      "api_name",
      "test",
      Acls {
        world: vec![PermissionFlag::Create, PermissionFlag::Read],
        ..Default::default()
      },
      AccessRules {
        // read: Some("(_ROW_._owner = _USER_.id OR EXISTS(SELECT 1 FROM room_members WHERE room =
        // _ROW_.room AND user = _USER_.id))".to_string()),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let record_id_raw = 0;
    let record_id = trailbase_sqlite::Value::Integer(record_id_raw);
    let rowid: i64 = conn
      .query_row(
        "INSERT INTO test (id, text) VALUES ($1, 'foo') RETURNING _rowid_",
        [record_id],
      )
      .await
      .unwrap()
      .unwrap()
      .get(0)
      .unwrap();

    assert_eq!(rowid, record_id_raw);

    let manager = state.subscription_manager();
    let api = state.lookup_record_api("api_name").unwrap();
    let receiver = manager
      .add_subscription(api, Some(trailbase_sqlite::Value::Integer(0)), None)
      .await
      .unwrap();

    assert_eq!(1, manager.num_subscriptions());

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
    match receiver.recv().await.unwrap() {
      DbEvent::Update(Some(value)) => {
        assert_eq!(value, expected);
      }
      x => {
        assert!(false, "Expected update, got: {x:?}");
      }
    };

    conn
      .execute("DELETE FROM test WHERE _rowid_ = $2", params!(rowid))
      .await
      .unwrap();

    match receiver.recv().await.unwrap() {
      DbEvent::Delete(Some(value)) => {
        assert_eq!(value, expected);
      }
      x => {
        assert!(false, "Expected update, got: {x:?}");
      }
    }

    assert_eq!(0, manager.num_subscriptions());
  }

  // TODO: Test actual SSE handler.
}

const NO_HOOK: Option<fn(Action, &str, &str, &PreUpdateCase)> = None;
