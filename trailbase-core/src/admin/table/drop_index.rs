use axum::{
  Json,
  extract::State,
  http::StatusCode,
  response::{IntoResponse, Response},
};
use log::*;
use serde::Deserialize;
use trailbase_schema::sqlite::QualifiedName;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::transaction::TransactionRecorder;

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct DropIndexRequest {
  pub name: String,
}

pub async fn drop_index_handler(
  State(state): State<AppState>,
  Json(request): Json<DropIndexRequest>,
) -> Result<Response, Error> {
  let index_name = QualifiedName::parse(&request.name)?;
  if state.demo_mode() && index_name.name.starts_with("_") {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }
  let filename = index_name.migration_filename("drop_index");

  let conn = state.conn();
  let log = conn
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

  // Write to migration file.
  if let Some(log) = log {
    let migration_path = state.data_dir().migrations_path();
    let _report = log
      .apply_as_migration(conn, migration_path, &filename)
      .await?;
  }

  return Ok((StatusCode::OK, "").into_response());
}
