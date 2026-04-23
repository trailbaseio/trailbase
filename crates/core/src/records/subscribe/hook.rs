use log::*;
use rusqlite::hooks::{Action, PreUpdateCase};
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
pub struct PreupdateHookEvent {
  pub action: RecordAction,
  pub table_name: QualifiedName,
  pub row_id: i64,
  // The actual record cells, i.e. a value per column.
  pub record: Vec<Value>,
}

pub fn uninstall_hook(conn: &Connection) -> Result<(), RecordError> {
  let lock = conn
    .write_lock()
    .map_err(|err| RecordError::Internal(err.into()))?;

  return lock.preupdate_hook(NO_HOOK).map_err(|err| {
    // If we were able to install a hook, we should also be able to uninstall it.
    log::error!("Failed to uninstall SQLite preupdate hook: {err}");
    RecordError::Internal(err.into())
  });
}

pub fn install_hook(
  conn: &Connection,
) -> Result<flume::Receiver<(usize, PreupdateHookEvent)>, RecordError> {
  let (sender, receiver) = flume::bounded(CAPACITY);
  let lock = conn
    .write_lock()
    .map_err(|err| RecordError::Internal(err.into()))?;

  lock
    .preupdate_hook({
      let conn = conn.clone();
      let mut cnt = 0;

      Some(
        move |action: Action, db: &str, table_name: &str, case: &PreUpdateCase| {
          // NOTE: We should do here as little work as possible. Specifially we don't do any
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

          let event = PreupdateHookEvent {
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
          };

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
        },
      )
    })
    .map_err(|err| RecordError::Internal(err.into()))?;

  return Ok(receiver);
}

const CAPACITY: usize = 16 * 1024;
const NO_HOOK: Option<fn(Action, &str, &str, &PreUpdateCase)> = None;

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn hook_test() {
    let conn = Connection::open_in_memory().unwrap();
    conn
      .execute_batch(
        "
          CREATE TABLE test (
            id    INTEGER PRIMARY KEY
          ) STRICT;
        ",
      )
      .await
      .unwrap();

    let receiver = install_hook(&conn).unwrap();

    conn
      .execute_batch(
        "
          INSERT INTO test (id) VALUES (3), (4);
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
}
