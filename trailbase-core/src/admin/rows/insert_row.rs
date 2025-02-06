use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::json_to_sql::{InsertQueryBuilder, JsonRow, Params};
use crate::records::sql_to_json::row_to_json_array;

#[derive(Debug, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct InsertRowRequest {
  /// Row data, which is expected to be a map from column name to value.
  pub row: JsonRow,
}

pub async fn insert_row_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Json(request): Json<InsertRowRequest>,
) -> Result<(), Error> {
  let _row = insert_row(&state, table_name, request.row).await?;
  return Ok(());
}

pub(crate) async fn insert_row(
  state: &AppState,
  table_name: String,
  json_row: JsonRow,
) -> Result<Vec<serde_json::Value>, Error> {
  let Some(table_metadata) = state.table_metadata().get(&table_name) else {
    return Err(Error::Precondition(format!("Table {table_name} not found")));
  };

  let row = InsertQueryBuilder::run(
    state,
    Params::from(&table_metadata, json_row, None)?,
    None,
    Some("*"),
    |row| Ok(trailbase_sqlite::Row::from_row(row, None)?),
  )
  .await?;

  return Ok(row_to_json_array(&row)?);
}
