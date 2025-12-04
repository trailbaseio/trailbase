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
  if state.demo_mode() {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let dry_run = request.dry_run.unwrap_or(false);
  let index_name = QualifiedName::parse(&request.name)?;

  let (conn, migration_path) =
    super::get_conn_and_migration_path(&state, index_name.database_schema.clone())?;

  let tx_log = {
    let unqualified_index_name = index_name.name.clone();
    conn
      .call(move |conn| {
        let mut tx = TransactionRecorder::new(conn)?;

        let query = format!("DROP INDEX IF EXISTS \"{unqualified_index_name}\"");
        debug!("dropping index: {query}");
        tx.execute(&query, ())?;

        return tx
          .rollback()
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await?
  };

  // Take transaction log, write a migration file and apply.
  if !dry_run && let Some(ref log) = tx_log {
    let filename = QualifiedName {
      name: index_name.name,
      database_schema: None,
    }
    .migration_filename("drop_index");

    let _report = log
      .apply_as_migration(&conn, migration_path, &filename)
      .await?;
  }

  return Ok(Json(DropIndexResponse {
    sql: tx_log.map(|l| l.build_sql()).unwrap_or_default(),
  }));
}
