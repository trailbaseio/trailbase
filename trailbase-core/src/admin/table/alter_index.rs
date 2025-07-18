use axum::extract::{Json, State};
use log::*;
use serde::{Deserialize, Serialize};
use trailbase_schema::sqlite::TableIndex;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::transaction::TransactionRecorder;

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct AlterIndexRequest {
  pub source_schema: TableIndex,
  pub target_schema: TableIndex,
  pub dry_run: Option<bool>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AlterIndexResponse {
  pub sql: String,
}

// NOTE: sqlite has very limited alter table support, thus we're always recreating the table and
// moving data over, see https://sqlite.org/lang_altertable.html.

pub async fn alter_index_handler(
  State(state): State<AppState>,
  Json(request): Json<AlterIndexRequest>,
) -> Result<Json<AlterIndexResponse>, Error> {
  if state.demo_mode() && request.source_schema.name.name.starts_with("_") {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let dry_run = request.dry_run.unwrap_or(false);
  let source_schema = request.source_schema;
  let source_index_name = source_schema.name.clone();
  let target_schema = request.target_schema;
  let filename = source_index_name.migration_filename("alter_index");

  debug!("Alter index:\nsource: {source_schema:?}\ntarget: {target_schema:?}",);

  if source_schema == target_schema {
    return Ok(Json(AlterIndexResponse {
      sql: "".to_string(),
    }));
  }

  let tx_log = state
    .conn()
    .call(move |conn| {
      let mut tx = TransactionRecorder::new(conn)?;

      // Drop old index
      tx.execute(
        &format!(
          "DROP INDEX {source_index_name}",
          source_index_name = source_index_name.escaped_string()
        ),
        (),
      )?;

      // Create new index
      let create_index_query = target_schema.create_index_statement();
      tx.execute(&create_index_query, ())?;

      return tx
        .rollback()
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
    })
    .await?;

  if !dry_run {
    // Take transaction log, write a migration file and apply.
    if let Some(ref log) = tx_log {
      let migration_path = state.data_dir().migrations_path();
      let report = log
        .apply_as_migration(state.conn(), migration_path, &filename)
        .await?;
      debug!("Migration report: {report:?}");
    }
  }

  return Ok(Json(AlterIndexResponse {
    sql: tx_log.map(|l| l.build_sql()).unwrap_or_default(),
  }));
}
