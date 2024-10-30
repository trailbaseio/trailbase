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
use crate::schema::TableIndex;
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
  let conn = state.conn();

  let source_schema = request.source_schema;
  let source_index_name = &source_schema.name;
  let target_schema = request.target_schema;

  debug!("Alter index:\nsource: {source_schema:?}\ntarget: {target_schema:?}",);

  let mut tx = TransactionRecorder::new(
    conn.clone(),
    state.data_dir().migrations_path(),
    format!("alter_index_{source_index_name}"),
  )
  .await?;

  // Drop old index
  tx.execute(&format!("DROP INDEX {source_index_name}"))
    .await?;

  // Create new index
  let create_index_query = target_schema.create_index_statement();
  tx.query(&create_index_query).await?;

  // Write to migration file.
  let report = tx.commit_and_create_migration().await?;
  debug!("Migration report: {report:?}");

  return Ok((StatusCode::OK, "altered index").into_response());
}
