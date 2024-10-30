use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::schema::Table;
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
  let conn = state.conn();
  if request.schema.columns.is_empty() {
    return Err(Error::Precondition(
      "Tables need to have at least one column".to_string(),
    ));
  }
  let dry_run = request.dry_run.unwrap_or(false);
  let table_name = request.schema.name.clone();

  // This contains the create table statement and may also contain indexes and triggers.
  let query = request.schema.create_table_statement();

  if !dry_run {
    let mut tx = TransactionRecorder::new(
      conn.clone(),
      state.data_dir().migrations_path(),
      format!("create_table_{table_name}"),
    )
    .await?;

    tx.query(&query).await?;

    // Write to migration file.
    tx.commit_and_create_migration().await?;

    state.table_metadata().invalidate_all().await?;
  }

  return Ok(Json(CreateTableResponse {
    sql: sqlformat::format(
      format!("{query};").as_str(),
      &sqlformat::QueryParams::None,
      sqlformat::FormatOptions {
        indent: sqlformat::Indent::Spaces(2),
        uppercase: true,
        lines_between_queries: 1,
      },
    ),
  }));
}
