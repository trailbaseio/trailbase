use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::backup;

#[derive(Debug, Deserialize, Serialize, TS)]
pub struct Backup {
  timestamp: String,
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
          timestamp: b.timestamp.to_rfc3339(),
        };
      })
      .collect(),
  }));
}

pub async fn trigger_backup_handler(
  State(state): State<AppState>,
) -> Result<Json<ListBackupsResponse>, Error> {
  let data_dir = state.data_dir();

  let result =
    crate::backup::backup_all(data_dir, &state.connection_manager(), &state.get_config()).await;

  if let Err(err) = crate::backup::delete_backups(data_dir, 5).await {
    log::warn!("Failed to clean-up backups: {err}");
  }

  result?;

  return Ok(Json(ListBackupsResponse {
    backups: backup::find_backups(state.data_dir())
      .await?
      .into_iter()
      .map(|b| {
        return Backup {
          timestamp: b.timestamp.to_rfc3339(),
        };
      })
      .collect(),
  }));
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct DeleteBackupsRequest {
  timestamps: Vec<String>,
}

pub async fn delete_backups_handler(
  State(state): State<AppState>,
  Json(request): Json<DeleteBackupsRequest>,
) -> Result<(), Error> {
  let backup_dir = state.data_dir().backup_path();
  for ts in request.timestamps {
    tokio::fs::remove_dir_all(backup_dir.join(ts))
      .await
      .map_err(|err| Error::Other(err.to_string()))?;
  }

  return Ok(());
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct RestoreBackupRequest {
  timestamp: String,
}

pub async fn restore_backup_handler(
  State(state): State<AppState>,
  Json(request): Json<RestoreBackupRequest>,
) -> Result<(), Error> {
  let timestamp = chrono::DateTime::parse_from_rfc3339(&request.timestamp)
    .map_err(|err| Error::Precondition(err.to_string()))?;

  let backup = backup::Backup {
    path: state.data_dir().backup_path().join(request.timestamp),
    timestamp: timestamp.into(),
  };

  backup::restore_all(state.data_dir(), &backup).await?;

  return Ok(());
}
