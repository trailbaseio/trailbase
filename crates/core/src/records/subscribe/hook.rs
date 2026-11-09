use log::*;
use parking_lot::{Mutex, RwLock};
use rusqlite::hooks::{Action, PreUpdateCase};
use std::sync::{Arc, Weak};
use trailbase_schema::QualifiedName;
use trailbase_sqlite::{
  Connection, Value,
  sqlite::{extract_record_values, extract_row_id},
};

use crate::records::RecordError;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum RecordAction {
  Delete,
  Insert,
  Update,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommittedEvent {
  pub action: RecordAction,
  pub table_name: QualifiedName,
  pub row_id: i64,
  // The actual record cells, i.e. a value per column.
  pub record: Vec<Value>,
}

pub enum BrokerEvent {
  Event(CommittedEvent),
  EventLoss,
}

fn uninstall_hook(conn: &Connection) -> Result<(), RecordError> {
  let mut lock = conn
    .write_lock()
    .map_err(|err| RecordError::Internal(err.into()))?;

  lock.preupdate_hook(NO_PREUPDATE_HOOK).map_err(|err| {
    // If we were able to install a hook, we should also be able to uninstall it.
    error!("Failed to uninstall SQLite preupdate hook: {err}");
    RecordError::Internal(err.into())
  })?;

  lock
    .rollback_hook(NO_ROLLBACK_HOOK)
    .map_err(|err| RecordError::Internal(err.into()))?;

  lock
    .commit_hook(NO_COMMIT_HOOK)
    .map_err(|err| RecordError::Internal(err.into()))?;

  return Ok(());
}

fn install_hook(
  conn: &Connection,
) -> Result<flume::Receiver<(usize, CommittedEvent)>, RecordError> {
  let (sender, receiver) = flume::bounded(CAPACITY);
  let mut lock = conn
    .write_lock()
    .map_err(|err| RecordError::Internal(err.into()))?;

  let buffer = Arc::new(Mutex::new(Vec::new()));

  lock
    .commit_hook(Some({
      let conn = conn.clone();
      let buffer = buffer.clone();
      let mut cnt = 0;

      move || {
        for event in buffer.lock().drain(..) {
          cnt += 1;

          match sender.try_send((cnt, event)) {
            Ok(()) => {}
            Err(flume::TrySendError::Full(_)) => {
              warn!("Channel full. Failed to forward preupdate event.");
            }
            Err(flume::TrySendError::Disconnected(_)) => {
              // QUESTION: Should it self-uninstall? This may be racy if a new hook
              // is being installed while one is already installed. In principle this
              // should not happen.
              let _ = uninstall_hook(&conn);
            }
          };
        }

        return false;
      }
    }))
    .map_err(|err| RecordError::Internal(err.into()))?;

  lock
    .rollback_hook(Some({
      let buffer = buffer.clone();

      move || {
        buffer.lock().clear();
      }
    }))
    .map_err(|err| RecordError::Internal(err.into()))?;

  lock
    .preupdate_hook({
      let buffer = buffer.clone();

      Some(
        move |action: Action, db: &str, table_name: &str, case: &PreUpdateCase| {
          // NOTE: We should do here as little work as possible. Specifically we don't do any
          // filtering here. This should be done by the receiver.
          let action = match action {
            Action::SQLITE_UPDATE => RecordAction::Update,
            Action::SQLITE_INSERT => RecordAction::Insert,
            Action::SQLITE_DELETE => RecordAction::Delete,
            a => {
              warn!("Skipping unknown SQLite action: {a:?}");
              return;
            }
          };

          let Some(row_id) = extract_row_id(case) else {
            warn!("Failed to extract row id");
            return;
          };

          let Some(record) = extract_record_values(case) else {
            warn!("Failed to extract values");
            return;
          };

          buffer.lock().push(CommittedEvent {
            action,
            table_name: QualifiedName {
              name: table_name.to_string(),
              database_schema: if db == "main" {
                None
              } else {
                Some(db.to_string())
              },
            },
            row_id,
            record,
          });
        },
      )
    })
    .map_err(|err| RecordError::Internal(err.into()))?;

  return Ok(receiver);
}

struct HookState {
  conn: Connection,
}

impl HookState {
  fn new(
    conn: Connection,
    broker: Arc<dyn Fn(BrokerEvent) + Sync + Send>,
  ) -> Result<Self, RecordError> {
    let receiver = install_hook(&conn)?;

    // Spawn broker task.
    let conn_id = conn.id();
    let _join_handle = std::thread::Builder::new()
      .name(format!("subscription_hook_{conn_id}"))
      .spawn(move || {
        debug!("Subscription broker task started, conn={conn_id}");

        let mut expected = 1;
        while let Ok((cnt, event)) = receiver.recv() {
          if cnt != expected {
            // QUESTION: There's several ways we could deal with event loss. We probably should
            // not  create back pressure on the `preupdate_hook` and gunk up SQLite access. We
            // could try to deliver event loss messages to all receivers but that may just make
            // the problem worse.
            broker(BrokerEvent::EventLoss);
            break;
          }

          expected += 1;

          broker(BrokerEvent::Event(event));
        }

        debug!("Channel closed: terminating subscription broker task.");
      })
      .map_err(|err| RecordError::Internal(err.into()))?;

    return Ok(Self { conn });
  }
}

impl Drop for HookState {
  fn drop(&mut self) {
    // Note: uninstalling the hooks will drop the senders, which will in turn terminate the broker
    // thread.
    if let Err(err) = uninstall_hook(&self.conn) {
      error!("Failed to uninstall hook: {err}");
    }
  }
}

/// An active handle keeps the hook alive.
pub struct HookHandle {
  _state: Arc<HookState>,
}

pub struct WeakHookHandle {
  conn: Connection,
  broker: Arc<dyn Fn(BrokerEvent) + Sync + Send>,
  state: RwLock<Weak<HookState>>,
}

impl WeakHookHandle {
  pub fn new(conn: Connection, broker: impl Fn(BrokerEvent) + Sync + Send + 'static) -> Self {
    return Self {
      conn,
      broker: Arc::new(broker),
      state: RwLock::new(Weak::new()),
    };
  }

  pub fn handle(&self) -> Result<HookHandle, RecordError> {
    let mut lock = self.state.upgradable_read();
    if let Some(state) = lock.upgrade() {
      return Ok(HookHandle { _state: state });
    }

    let state = lock.with_upgraded(|lock| -> Result<Arc<HookState>, RecordError> {
      let new_state = Arc::new(HookState::new(self.conn.clone(), self.broker.clone())?);
      *lock = Arc::downgrade(&new_state);
      return Ok(new_state);
    })?;

    return Ok(HookHandle { _state: state });
  }
}

const NO_PREUPDATE_HOOK: Option<fn(Action, &str, &str, &PreUpdateCase)> = None;
const NO_ROLLBACK_HOOK: Option<fn()> = None;
const NO_COMMIT_HOOK: Option<fn() -> bool> = None;
const CAPACITY: usize = 16 * 1024;

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn hook_test() {
    let conn = Connection::open_in_memory().unwrap();
    conn
      .execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY) STRICT;")
      .await
      .unwrap();

    let receiver = install_hook(&conn).unwrap();

    // Make sure, the rolled back transaction doesn't make it.
    conn
      .execute_batch(
        "
          BEGIN;
          INSERT INTO test (id) VALUES (1), (2);
          ROLLBACK;
        ",
      )
      .await
      .unwrap();

    conn
      .execute_batch(
        "
          BEGIN;
          INSERT INTO test (id) VALUES (3), (4);
          COMMIT;
        ",
      )
      .await
      .unwrap();

    let (cnt, ev0) = receiver.recv_async().await.unwrap();
    assert_eq!(1, cnt);
    assert_eq!("\"test\"", ev0.table_name.escaped_string());
    assert_eq!(Value::Integer(3), ev0.record[0]);

    let (cnt, ev1) = receiver.recv_async().await.unwrap();
    assert_eq!(2, cnt);
    assert_eq!(Value::Integer(4), ev1.record[0]);

    uninstall_hook(&conn).unwrap();

    assert_eq!(0, receiver.sender_count());
  }

  #[tokio::test]
  async fn hook_lifecycle() {
    let conn = Connection::open_in_memory().unwrap();
    conn
      .execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY) STRICT;")
      .await
      .unwrap();

    conn
      .execute_batch("INSERT INTO test (id) VALUES (1), (2);")
      .await
      .unwrap();

    let (sender, receiver) = flume::bounded(CAPACITY);
    let weak = WeakHookHandle::new(conn.clone(), move |event| {
      sender.send(event).unwrap();
    });

    assert_eq!(0, weak.state.read().strong_count());

    {
      let strong = weak.handle().unwrap();

      conn
        .execute_batch("INSERT INTO test (id) VALUES (3);")
        .await
        .unwrap();

      drop(strong);
    }

    assert_eq!(0, weak.state.read().strong_count());

    conn
      .execute_batch("INSERT INTO test (id) VALUES (4);")
      .await
      .unwrap();

    let received: Vec<_> = receiver.drain().collect();

    assert_eq!(1, received.len());

    let BrokerEvent::Event(ref event) = received[0] else {
      panic!("expected event");
    };

    assert_eq!(
      QualifiedName {
        name: "test".to_string(),
        database_schema: None
      },
      event.table_name
    );
    assert_eq!(Value::Integer(3), event.record[0]);
  }
}
