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

pub async fn alter_index_handler(
  State(state): State<AppState>,
  Json(request): Json<AlterIndexRequest>,
) -> Result<Json<AlterIndexResponse>, Error> {
  if state.demo_mode() {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let dry_run = request.dry_run.unwrap_or(false);

  debug!(
    "Alter index:\nsource: {:?}\ntarget: {:?}",
    request.source_schema, request.target_schema
  );

  if request.source_schema == request.target_schema {
    return Ok(Json(AlterIndexResponse {
      sql: "".to_string(),
    }));
  }

  let (db, target_index_schema) = {
    let mut schema = request.target_schema.clone();
    (schema.name.database_schema.take(), schema)
  };

  let (conn, migration_path) = super::get_conn_and_migration_path(&state, db)?;

  let tx_log = {
    let unqualified_source_index_name = request.source_schema.name.name.clone();
    let create_index_query = target_index_schema.create_index_statement();

    conn
      .call(move |conn| {
        let mut tx = TransactionRecorder::new(conn)?;

        // Drop old index
        tx.execute(&format!("DROP INDEX {unqualified_source_index_name}"), ())?;

        // Create new index
        tx.execute(&create_index_query, ())?;

        return tx
          .rollback()
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await?
  };

  // Take transaction log, write a migration file and apply.
  if !dry_run && let Some(ref log) = tx_log {
    let filename = target_index_schema.name.migration_filename("alter_index");

    let report = log
      .apply_as_migration(&conn, migration_path, &filename)
      .await?;
    debug!("Migration report: {report:?}");
  }

  return Ok(Json(AlterIndexResponse {
    sql: tx_log.map(|l| l.build_sql()).unwrap_or_default(),
  }));
}
