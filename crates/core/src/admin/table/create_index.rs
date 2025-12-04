use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use trailbase_schema::sqlite::TableIndex;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::transaction::TransactionRecorder;

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateIndexRequest {
  pub schema: TableIndex,
  pub dry_run: Option<bool>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct CreateIndexResponse {
  pub sql: String,
}

pub async fn create_index_handler(
  State(state): State<AppState>,
  Json(request): Json<CreateIndexRequest>,
) -> Result<Json<CreateIndexResponse>, Error> {
  let dry_run = request.dry_run.unwrap_or(false);
  let (db, index_schema) = {
    let mut schema = request.schema.clone();
    (schema.name.database_schema.take(), schema)
  };

  // This builds the `CREATE INDEX` SQL statement.
  let create_index_query = request.schema.create_index_statement();

  let (conn, migration_path) = super::get_conn_and_migration_path(&state, db)?;

  let tx_log = conn
    .call(move |conn| {
      let mut tx = TransactionRecorder::new(conn)?;

      tx.execute(&create_index_query, ())?;

      return tx
        .rollback()
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
    })
    .await?;

  // Take transaction log, write a migration file and apply.
  if !dry_run && let Some(ref log) = tx_log {
    let filename = index_schema.name.migration_filename("create_index");
    log
      .apply_as_migration(&conn, migration_path, &filename)
      .await?;
  }

  return Ok(Json(CreateIndexResponse {
    sql: tx_log.map(|l| l.build_sql()).unwrap_or_default(),
  }));
}
