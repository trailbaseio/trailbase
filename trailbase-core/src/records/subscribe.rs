use axum::{
  extract::{Path, State},
  response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{Stream, StreamExt};
use parking_lot::{Mutex, RwLock};
use rusqlite::hooks::{Action, PreUpdateCase};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{
  atomic::{AtomicI64, Ordering},
  Arc,
};
use trailbase_sqlite::params;

use crate::auth::user::User;
use crate::records::RecordApi;
use crate::records::{Permission, RecordError};
use crate::AppState;

static SUBSCRIPTION_COUNTER: AtomicI64 = AtomicI64::new(0);

// TODO:
//  * clients
//  * table-wide subscriptions
//  * opt: avoid repeated encoding of events

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
  record_id: Option<trailbase_sqlite::Value>,

  api: RecordApi,
  user: Option<User>,
  channel: async_channel::Sender<DbEvent>,
}

struct ManagerState {
  conn: trailbase_sqlite::Connection,

  subscriptions: RwLock<HashMap<String, HashMap<i64, Vec<Subscription>>>>,
  installed: Mutex<bool>,
}

#[derive(Clone)]
pub struct SubscriptionManager {
  state: Arc<ManagerState>,
}

impl SubscriptionManager {
  pub fn new(conn: trailbase_sqlite::Connection) -> Result<Self, crate::InitError> {
    return Ok(Self {
      state: Arc::new(ManagerState {
        conn,
        subscriptions: RwLock::new(HashMap::new()),
        installed: Mutex::new(false),
      }),
    });
  }

  fn remove_hook(state: &ManagerState, conn: &rusqlite::Connection) {
    let mut installed = state.installed.lock();
    if !*installed {
      return;
    }

    *installed = false;

    conn.update_hook(None::<fn(rusqlite::hooks::Action, &str, &str, i64)>);
  }

  fn hook(
    s: &Arc<ManagerState>,
    conn: &rusqlite::Connection,
    action: Action,
    db: &str,
    table: &str,
    rowid: i64,
  ) {
    assert_eq!(db, "main");

    match action {
      Action::SQLITE_UPDATE | Action::SQLITE_INSERT | Action::SQLITE_DELETE => {}
      a => {
        log::error!("Unknown action: {a:?}");
        return;
      }
    };
    let action: RecordAction = action.into();

    let mut cleanups: Vec<usize> = vec![];
    {
      let lock = s.subscriptions.read();
      if let Some(subs) = lock.get(table).and_then(|m| m.get(&rowid)) {
        let event = match action {
          RecordAction::Delete => DbEvent::Delete(None),
          RecordAction::Insert => DbEvent::Insert(None),
          RecordAction::Update => DbEvent::Update(None),
        };

        for (idx, sub) in subs.iter().enumerate() {
          let api = &sub.api;
          if let Err(_err) = api.check_record_level_read_access_by_rowid(
            conn,
            Permission::Read,
            rowid,
            sub.user.as_ref(),
          ) {
            let _ = sub.channel.try_send(DbEvent::Error("Access denied".into()));
            continue;
          }

          if let Err(err) = sub.channel.try_send(event.clone()) {
            match err {
              async_channel::TrySendError::Full(ev) => {
                log::warn!("Channel full, dropping event: {ev:?}");
              }
              async_channel::TrySendError::Closed(_ev) => {
                cleanups.push(idx);
              }
            }
          }
        }
      }
    }

    if !cleanups.is_empty() {
      let mut lock = s.subscriptions.write();

      if let Some(r) = lock.get_mut(table) {
        if let Some(m) = r.get_mut(&rowid) {
          for idx in cleanups.iter().rev() {
            m.swap_remove(*idx);
          }

          if m.is_empty() {
            r.remove(&rowid);
          }
        }

        if r.is_empty() {
          lock.remove(table);
        }
      }

      if lock.is_empty() {
        Self::remove_hook(s, conn);
      }
    }

    // Cleanup subscriptions on delete.
    if action == RecordAction::Delete {
      let mut lock = s.subscriptions.write();
      if let Some(m) = lock.get_mut(table) {
        m.remove(&rowid);

        if m.is_empty() {
          lock.remove(table);
        }
      }

      if lock.is_empty() {
        Self::remove_hook(s, conn);
      }
    }
  }

  async fn add_hook(state: Arc<ManagerState>) -> trailbase_sqlite::connection::Result<()> {
    {
      let mut installed = state.installed.lock();
      if *installed {
        return Ok(());
      }
      *installed = true;
    }

    let s = state.clone();

    state
      .conn
      .call(|conn| {
        conn.preupdate_hook(Some(
          |action: Action, db: &str, table: &str, value: &PreUpdateCase| {
            // TODO: explore this as a more perfomant alternative to post-update hook with rowid and
            // additional lookups. Security/ACL implications?
          },
        ));

        return Ok(());
      })
      .await?;

    return state
      .conn
      .add_hook(
        move |conn: &rusqlite::Connection, action: Action, db: &str, table: &str, rowid: i64| {
          Self::hook(&s, conn, action, db, table, rowid);
        },
      )
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
    {
      let mut lock = self.state.subscriptions.write();
      let m: &mut HashMap<i64, Vec<Subscription>> = lock.entry(table_name.to_string()).or_default();

      m.entry(row_id).or_default().push(Subscription {
        subscription_id,
        api,
        record_id: Some(record),
        user,
        channel: sender,
      });
    }

    let installed: bool = *self.state.installed.lock();
    if !installed {
      Self::add_hook(self.state.clone()).await.unwrap();
    }

    return Ok(receiver);
  }

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
  //     Self::remove_hook(&*self.state).await?;
  //   }
  //
  //   return Ok(());
  // }
}

pub async fn sse_handler(
  State(state): State<AppState>,
  Path((api_name, record)): Path<(String, String)>,
  user: Option<User>,
) -> Result<Sse<impl Stream<Item = Result<axum::response::sse::Event, axum::Error>>>, RecordError> {
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

  let mgr = state.subscription_manager().clone();
  let receiver = mgr.add_subscription(api, Some(record_id), user).await?;

  return Ok(
    Sse::new(receiver.map(|ev| Event::default().json_data(ev))).keep_alive(KeepAlive::default()),
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

    let manager = SubscriptionManager::new(conn.clone()).unwrap();
    let api = state.lookup_record_api("api_name").unwrap();
    let receiver = manager
      .add_subscription(api, Some(trailbase_sqlite::Value::Integer(0)), None)
      .await
      .unwrap();

    conn
      .execute(
        "UPDATE test SET text = $1 WHERE _rowid_ = $2",
        params!("bar", rowid),
      )
      .await
      .unwrap();

    conn
      .execute("DELETE FROM test WHERE _rowid_ = $2", params!(rowid))
      .await
      .unwrap();

    assert!(matches!(
      receiver.recv().await.unwrap(),
      DbEvent::Update(None)
    ));
    // assert!(matches!(receiver.recv().await.unwrap().action));
  }

  // TODO: Test actual SSE handler.
}
