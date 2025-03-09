use axum::{
  extract::State,
  http::StatusCode,
  response::{IntoResponse, Response},
  Json,
};
use log::*;
use serde::Deserialize;
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
  let index_name = request.name;
  if state.demo_mode() && index_name.starts_with("_") {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let migration_path = state.data_dir().migrations_path();
  let conn = state.conn();
  let writer = conn
    .call(move |conn| {
      let mut tx =
        TransactionRecorder::new(conn, migration_path, format!("drop_index_{index_name}"))?;

      let query = format!("DROP INDEX IF EXISTS {}", index_name);
      info!("dropping index: {query}");
      tx.execute(&query)?;

      return tx
        .rollback_and_create_migration()
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
    })
    .await?;

  // Write to migration file.
  if let Some(writer) = writer {
    let _report = writer.write(conn).await?;
  }

  return Ok((StatusCode::OK, "").into_response());
}
