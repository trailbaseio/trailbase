use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::backup;

#[derive(Debug, Deserialize, Serialize, TS)]
pub struct Backup {
  timestamp: i64,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct ListBackupsResponse {
  backups: Vec<Backup>,
}

pub async fn list_backups_handler(
  State(state): State<AppState>,
) -> Result<Json<ListBackupsResponse>, Error> {
  return Ok(Json(ListBackupsResponse {
    backups: backup::find_backups(state.data_dir())
      .await?
      .into_iter()
      .map(|b| {
        return Backup {
          timestamp: b.timestamp.timestamp(),
        };
      })
      .collect(),
  }));
}

pub async fn trigger_backup_handler(
  State(state): State<AppState>,
) -> Result<Json<ListBackupsResponse>, Error> {
  let backup_window_size =
    state.access_config(|c| c.server.backup_window_size.unwrap_or(5)) as usize;
  if backup_window_size == 0 {
    return Err(Error::Precondition(
      "Backups disabled. Window size explicitly set to 0".into(),
    ));
  }

  let data_dir = state.data_dir();

  let result =
    crate::backup::backup_all(data_dir, &state.connection_manager(), &state.get_config()).await;

  if let Err(err) = crate::backup::delete_backups(data_dir, backup_window_size).await {
    log::warn!("Failed to clean-up backups: {err}");
  }

  result?;

  return Ok(Json(ListBackupsResponse {
    backups: backup::find_backups(state.data_dir())
      .await?
      .into_iter()
      .map(|b| {
        return Backup {
          timestamp: b.timestamp.timestamp(),
        };
      })
      .collect(),
  }));
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct DeleteBackupsRequest {
  timestamps: Vec<i64>,
}

pub async fn delete_backups_handler(
  State(state): State<AppState>,
  Json(request): Json<DeleteBackupsRequest>,
) -> Result<(), Error> {
  let backup_dir = state.data_dir().backup_path();
  for ts in request.timestamps {
    let instant = chrono::DateTime::from_timestamp(ts, 0)
      .ok_or_else(|| Error::Precondition("invalid timestamp".into()))?;

    tokio::fs::remove_dir_all(backup_dir.join(instant.timestamp().to_string()))
      .await
      .map_err(|err| Error::Other(err.to_string()))?;
  }

  return Ok(());
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct RestoreBackupRequest {
  timestamp: i64,
}

pub async fn restore_backup_handler(
  State(state): State<AppState>,
  Json(request): Json<RestoreBackupRequest>,
) -> Result<(), Error> {
  let instant = chrono::DateTime::from_timestamp(request.timestamp, 0)
    .ok_or_else(|| Error::Precondition("invalid timestamp".into()))?;

  let backup = backup::Backup {
    path: state
      .data_dir()
      .backup_path()
      .join(instant.timestamp().to_string()),
    timestamp: instant,
  };

  backup::restore_all(state.data_dir(), &backup).await?;

  return Ok(());
}
