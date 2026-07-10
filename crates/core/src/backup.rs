use std::path::PathBuf;
use thiserror::Error;

use crate::config::proto::Config;
use crate::connection::{BuildOptions, ConnectionError, ConnectionManager};
use crate::{AppState, DataDir};

#[derive(Debug, Error)]
pub enum BackupError {
  #[error("Connection: {0}")]
  Connection(#[from] ConnectionError),
  #[error("Filesystem: {0}")]
  Filesystem(#[from] std::io::Error),
  #[error("Backups: {0:?}")]
  Backups(Vec<trailbase_sqlite::Error>),
  #[error("Other: {0}")]
  Other(String),
}

pub async fn backup_all(
  data_dir: &DataDir,
  mgr: &ConnectionManager,
  config: &Config,
) -> Result<(), BackupError> {
  if !matches!(
    mgr.main_entry().connection.connection_type(),
    trailbase_sqlite::ConnectionType::Sqlite
  ) {
    return Err(BackupError::Other("Only sqlite supported for now".into()));
  }

  let attached_dbs: Vec<String> = config
    .record_apis
    .iter()
    .map(|c| c.attached_databases.clone())
    .flatten()
    .collect();

  let dbs: Vec<String> = [
    vec![
      "main".to_string(),
      "logs".to_string(),
      "session".to_string(),
    ],
    attached_dbs,
  ]
  .into_iter()
  .flatten()
  .collect();

  let now = chrono::Utc::now();
  let target_path = data_dir.backup_path().join(now.to_rfc3339());

  std::fs::create_dir_all(&target_path)?;

  let mut errors = vec![];
  for db in dbs {
    // let schema = if db == "main" { None } else { Some(db.clone()) };

    let conn = match connect_db(data_dir.data_path().join(format!("{db}.db"))) {
      Ok(conn) => conn,
      Err(err) => {
        log::warn!("Failed open '{db}' for backup: {err}");
        continue;
      }
    };

    if let Err(err) = conn.backup_to_dir(&target_path, None).await {
      log::warn!("backup failed for DB '{db}': {err}");
      errors.push(err)
    }
  }

  if errors.is_empty() {
    return Ok(());
  }

  return Err(BackupError::Backups(errors));
}

#[derive(Debug)]
pub struct Backup {
  pub path: PathBuf,
  pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub async fn restore_all(mgr: &ConnectionManager, backup: &Backup) -> Result<(), BackupError> {
  let dir = std::fs::read_dir(&backup.path)?;

  let dbs: Vec<_> = dir
    .into_iter()
    .flat_map(|entry| {
      let Ok(entry) = entry else {
        return None;
      };

      let Ok(metadata) = entry.metadata() else {
        return None;
      };

      if metadata.is_file() && !metadata.is_symlink() {
        let path = entry.path();
        let extension = path.extension()?;
        if extension == "db" {
          return None;
        }

        return Some(path);
      }

      return None;
    })
    .collect();

  let mut errors = vec![];
  for db in dbs {
    let conn = match connect_db(db.clone()) {
      Ok(conn) => conn,
      Err(err) => {
        log::warn!("Failed open '{db:?}' for restore: {err}");
        continue;
      }
    };

    if let Err(err) = conn.restore(backup.path.join(db), None).await {
      errors.push(err);
    }
  }

  if errors.is_empty() {
    return Ok(());
  }

  return Err(BackupError::Backups(errors));
}

pub async fn delete_backups(data_dir: &DataDir, keep: usize) -> Result<(), BackupError> {
  let mut backups = find_backups(data_dir)?;
  backups.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

  let mut n = backups.len();
  for backup in backups.iter().take_while(|_| {
    if n > keep {
      n -= 1;
      return true;
    }
    return false;
  }) {
    if let Err(err) = std::fs::remove_dir_all(&backup.path) {
      log::warn!("Failed to delete {backup:?}: {err}");
    }
  }

  return Ok(());
}

fn find_backups(data_dir: &DataDir) -> Result<Vec<Backup>, BackupError> {
  let dir = std::fs::read_dir(data_dir.backup_path())?;

  return Ok(
    dir
      .into_iter()
      .flat_map(|entry| {
        let Ok(entry) = entry else {
          return None;
        };

        let Ok(metadata) = entry.metadata() else {
          return None;
        };

        if metadata.is_dir() {
          let path = entry.path();
          let Some(last) = path.components().last() else {
            return None;
          };

          let Ok(timestamp) =
            chrono::DateTime::parse_from_rfc3339(&last.as_os_str().to_string_lossy())
          else {
            return None;
          };

          return Some(Backup {
            path,
            timestamp: timestamp.into(),
          });
        }

        return None;
      })
      .collect(),
  );
}

fn connect_db(path: PathBuf) -> Result<trailbase_sqlite::Connection, ConnectionError> {
  return trailbase_sqlite::Connection::with_opts(
    || -> Result<_, trailbase_sqlite::Error> {
      let conn = crate::connection::connect_rusqlite_without_default_extensions_and_schemas(Some(
        path.clone(),
      ))?;

      // NOTE: The many DBs (main, logs, ...) need the trailbase extensions, e.g. for the maxminddb geoip lookup.
      trailbase_extension::register_all_extension_functions(&conn, None)?;

      return Ok(conn);
    },
    trailbase_sqlite::Options {
      // Only using the writer, no readers (except for admin dash).
      num_threads: Some(1),
      ..Default::default()
    },
  )
  .map_err(ConnectionError::Sql);
}

#[cfg(all(test, not(feature = "pg-test")))]
mod tests {
  use super::*;
  use crate::app_state::test_state;

  #[tokio::test]
  async fn test_backup() {
    let state = test_state(None).await.unwrap();

    backup_all(
      state.data_dir(),
      &state.connection_manager(),
      &state.get_config(),
    )
    .await
    .unwrap();

    let backups = find_backups(state.data_dir()).unwrap();
    assert_eq!(1, backups.len());

    restore_all(&state.connection_manager(), &backups[0])
      .await
      .unwrap();
  }
}
