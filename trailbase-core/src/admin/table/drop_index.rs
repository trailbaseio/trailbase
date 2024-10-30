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
  let conn = state.conn();
  let index_name = request.name;

  let mut tx = TransactionRecorder::new(
    conn.clone(),
    state.data_dir().migrations_path(),
    format!("drop_index_{index_name}"),
  )
  .await?;

  let query = format!("DROP INDEX IF EXISTS {}", index_name);
  info!("dropping index: {query}");
  tx.execute(&query).await?;

  // Write to migration file.
  tx.commit_and_create_migration().await?;

  return Ok((StatusCode::OK, "").into_response());
}
