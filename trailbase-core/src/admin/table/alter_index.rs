use axum::{
  Json,
  extract::State,
  http::StatusCode,
  response::{IntoResponse, Response},
};
use log::*;
use serde::Deserialize;
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
}

// NOTE: sqlite has very limited alter table support, thus we're always recreating the table and
// moving data over, see https://sqlite.org/lang_altertable.html.

pub async fn alter_index_handler(
  State(state): State<AppState>,
  Json(request): Json<AlterIndexRequest>,
) -> Result<Response, Error> {
  if state.demo_mode() && request.source_schema.name.name.starts_with("_") {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let source_schema = request.source_schema;
  let source_index_name = source_schema.name.clone();
  let target_schema = request.target_schema;
  let filename = source_index_name.migration_filename("alter_index");

  debug!("Alter index:\nsource: {source_schema:?}\ntarget: {target_schema:?}",);

  let conn = state.conn();
  let log = conn
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

  // Write to migration file.
  if let Some(log) = log {
    let migration_path = state.data_dir().migrations_path();
    let report = log
      .apply_as_migration(conn, migration_path, &filename)
      .await?;
    debug!("Migration report: {report:?}");
  }

  return Ok((StatusCode::OK, "altered index").into_response());
}
