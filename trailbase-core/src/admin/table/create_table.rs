use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use trailbase_schema::sqlite::Table;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::transaction::TransactionRecorder;

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateTableRequest {
  pub schema: Table,
  pub dry_run: Option<bool>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct CreateTableResponse {
  pub sql: String,
}

pub async fn create_table_handler(
  State(state): State<AppState>,
  Json(request): Json<CreateTableRequest>,
) -> Result<Json<CreateTableResponse>, Error> {
  if request.schema.columns.is_empty() {
    return Err(Error::Precondition(
      "Tables need to have at least one column".to_string(),
    ));
  }
  let dry_run = request.dry_run.unwrap_or(false);
  let filename = request.schema.name.migration_filename("create_table");

  // This contains the create table statement and may also contain indexes and triggers.
  let create_table_query = request.schema.create_table_statement();

  let conn = state.conn();
  let tx_log = conn
    .call(move |conn| {
      let mut tx = TransactionRecorder::new(conn)?;

      tx.execute(&create_table_query, ())?;

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
        .apply_as_migration(conn, migration_path, &filename)
        .await?;
    }

    state.schema_metadata().invalidate_all().await?;
  }

  return Ok(Json(CreateTableResponse {
    sql: tx_log.map(|l| l.build_sql()).unwrap_or_default(),
  }));
}
