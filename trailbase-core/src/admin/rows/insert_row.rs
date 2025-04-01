use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::params::{JsonRow, Params};
use crate::records::query_builder::InsertQueryBuilder;

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
  let _row_id = insert_row(&state, table_name, request.row).await?;
  return Ok(());
}

pub(crate) async fn insert_row(
  state: &AppState,
  table_name: String,
  json_row: JsonRow,
) -> Result<i64, Error> {
  let Some(table_metadata) = state.table_metadata().get(&table_name) else {
    return Err(Error::Precondition(format!("Table {table_name} not found")));
  };

  let rowid_value = InsertQueryBuilder::run(
    state,
    &table_metadata,
    Params::from(&table_metadata, json_row, None)?,
    None,
    "_rowid_",
  )
  .await?;

  return match rowid_value {
    rusqlite::types::Value::Integer(rowid) => Ok(rowid),
    _ => Err(Error::Internal(
      format!("unexpected return type: {rowid_value:?}").into(),
    )),
  };
}
