use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::backup;

// TODO: Missing handlers
// * Delete specific backup(s).
// * Create a new backup.

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
  let backups = backup::find_backups(state.data_dir()).await?;

  return Ok(Json(ListBackupsResponse {
    backups: backups
      .into_iter()
      .map(|b| {
        return Backup {
          timestamp: b.timestamp.to_rfc3339(),
        };
      })
      .collect(),
  }));
}
