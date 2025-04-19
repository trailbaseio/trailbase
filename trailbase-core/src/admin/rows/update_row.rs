use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::params::{JsonRow, Params, simple_json_value_to_param};
use crate::records::query_builder::UpdateQueryBuilder;

#[derive(Debug, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct UpdateRowRequest {
  pub primary_key_column: String,

  #[ts(type = "Object")]
  pub primary_key_value: serde_json::Value,

  /// Row data, which is expected to be a map from column name to value.
  ///
  /// Note that the row is represented as a map to allow selective cells as opposed to
  /// Vec<serde_json::Value>. Absence is different from setting a column to NULL.
  pub row: JsonRow,
}

pub async fn update_row_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Json(request): Json<UpdateRowRequest>,
) -> Result<(), Error> {
  if state.demo_mode() && table_name.starts_with("_") {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let Some(table_metadata) = state.table_metadata().get(&table_name) else {
    return Err(Error::Precondition(format!("Table {table_name} not found")));
  };

  let pk_col = &request.primary_key_column;
  let Some((index, column)) = table_metadata.column_by_name(pk_col) else {
    return Err(Error::Precondition(format!("Missing column: {pk_col}")));
  };

  if let Some(pk_index) = table_metadata.record_pk_column {
    if index != pk_index {
      return Err(Error::Precondition(format!("Pk column mismatch: {pk_col}")));
    }
  }

  if !column.is_primary() {
    return Err(Error::Precondition(format!("Not a primary key: {pk_col}")));
  }

  UpdateQueryBuilder::run(
    &state,
    table_metadata.name(),
    &column.name,
    simple_json_value_to_param(column.data_type, request.primary_key_value)?,
    table_metadata.json_metadata.has_file_columns(),
    Params::from(&*table_metadata, request.row, None)?,
  )
  .await?;

  return Ok(());
}
