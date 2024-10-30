use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::schema::TableIndex;
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
  let conn = state.conn();
  let dry_run = request.dry_run.unwrap_or(false);
  let index_name = request.schema.name.clone();

  let create_index_query = request.schema.create_index_statement();

  if !dry_run {
    let mut tx = TransactionRecorder::new(
      conn.clone(),
      state.data_dir().migrations_path(),
      format!("create_index_{index_name}"),
    )
    .await?;

    tx.query(&create_index_query).await?;

    // Write to migration file.
    tx.commit_and_create_migration().await?;
  }

  return Ok(Json(CreateIndexResponse {
    sql: sqlformat::format(
      &format!("{create_index_query};"),
      &sqlformat::QueryParams::None,
      sqlformat::FormatOptions {
        indent: sqlformat::Indent::Spaces(2),
        uppercase: true,
        lines_between_queries: 1,
      },
    ),
  }));
}
