use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use trailbase_schema::{QualifiedName, QualifiedNameEscaped};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::params::{JsonRow, Params};
use crate::records::write_queries::run_update_query;

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

  let table_name = QualifiedName::parse(&table_name)?;
  let Some(schema_metadata) = state.schema_metadata().get_table(&table_name) else {
    return Err(Error::Precondition(format!(
      "Table {table_name:?} not found"
    )));
  };

  let pk_col = &request.primary_key_column;
  let Some((index, column)) = schema_metadata.column_by_name(pk_col) else {
    return Err(Error::Precondition(format!("Missing column: {pk_col}")));
  };

  if let Some(pk_index) = schema_metadata.record_pk_column {
    if index != pk_index {
      return Err(Error::Precondition(format!("Pk column mismatch: {pk_col}")));
    }
  }

  if !column.is_primary() {
    return Err(Error::Precondition(format!("Not a primary key: {pk_col}")));
  }

  run_update_query(
    &state,
    &QualifiedNameEscaped::new(&schema_metadata.schema.name),
    Params::for_update(
      &*schema_metadata,
      request.row,
      None,
      pk_col.clone(),
      // NOTE: We "fancy" parse JSON string values, since the UI currently ships everything as a
      // string. We could consider pushing some more type-awareness into the ui.
      trailbase_schema::json::flat_json_to_value(
        column.data_type,
        request.primary_key_value,
        true,
      )?,
      true,
    )?,
  )
  .await?;

  return Ok(());
}
