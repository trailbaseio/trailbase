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
  let (db, table_schema) = {
    let mut schema = request.schema.clone();
    (schema.name.database_schema.take(), schema)
  };

  let (conn, migration_path) = super::get_conn_and_migration_path(&state, db)?;

  // This builds the `CREATE TABLE` SQL statement.
  let create_table_query = table_schema.create_table_statement();

  let tx_log = conn
    .call(move |conn| {
      let mut tx = TransactionRecorder::new(conn)?;

      tx.execute(&create_table_query, ())?;

      return tx
        .rollback()
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
    })
    .await?;

  // Take transaction log, write a migration file and apply.
  if !dry_run && let Some(ref log) = tx_log {
    let filename = table_schema.name.migration_filename("create_table");
    let _report = log
      .apply_as_migration(&conn, migration_path, &filename)
      .await?;

    state.rebuild_connection_metadata().await?;
  }

  return Ok(Json(CreateTableResponse {
    sql: tx_log.map(|l| l.build_sql()).unwrap_or_default(),
  }));
}
