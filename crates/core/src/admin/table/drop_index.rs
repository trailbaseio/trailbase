use axum::extract::{Json, State};
use log::*;
use serde::{Deserialize, Serialize};
use trailbase_schema::sqlite::QualifiedName;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::transaction::TransactionRecorder;

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct DropIndexRequest {
  pub name: String,
  pub dry_run: Option<bool>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct DropIndexResponse {
  pub sql: String,
}

pub async fn drop_index_handler(
  State(state): State<AppState>,
  Json(request): Json<DropIndexRequest>,
) -> Result<Json<DropIndexResponse>, Error> {
  let index_name = QualifiedName::parse(&request.name)?;
  if state.demo_mode() {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let dry_run = request.dry_run.unwrap_or(false);
  let filename = index_name.migration_filename("drop_index");

  let tx_log = state
    .conn()
    .call(move |conn| {
      let mut tx = TransactionRecorder::new(conn)?;

      let query = format!(
        "DROP INDEX IF EXISTS {name}",
        name = index_name.escaped_string()
      );
      debug!("dropping index: {query}");
      tx.execute(&query, ())?;

      return tx
        .rollback()
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
    })
    .await?;

  if !dry_run {
    // Take transaction log, write a migration file and apply.
    if let Some(ref log) = tx_log {
      let migration_path = state.data_dir().migrations_path();
      let _report = log
        .apply_as_migration(state.conn(), migration_path, &filename)
        .await?;
    }
  }

  return Ok(Json(DropIndexResponse {
    sql: tx_log.map(|l| l.build_sql()).unwrap_or_default(),
  }));
}
