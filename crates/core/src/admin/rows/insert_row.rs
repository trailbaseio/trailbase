use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use trailbase_schema::{QualifiedName, QualifiedNameEscaped};
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
  let _row_id = insert_row(&state, QualifiedName::parse(&table_name)?, request.row).await?;
  return Ok(());
}

pub(crate) async fn insert_row(
  state: &AppState,
  table_name: QualifiedName,
  json_row: JsonRow,
) -> Result<i64, Error> {
  let Some(schema_metadata) = state.schema_metadata().get_table(&table_name) else {
    return Err(Error::Precondition(format!(
      "Table {table_name:?} not found"
    )));
  };

  let rowid_value = InsertQueryBuilder::run(
    state,
    &QualifiedNameEscaped::new(&schema_metadata.schema.name),
    None,
    "_rowid_",
    schema_metadata.json_metadata.has_file_columns(),
    // NOTE: We "fancy" parse JSON string values, since the UI currently ships everything as a
    // string. We could consider pushing some more type-awareness into the ui.
    Params::for_insert(&*schema_metadata, json_row, None, true)?,
  )
  .await?;

  return match rowid_value {
    rusqlite::types::Value::Integer(rowid) => Ok(rowid),
    _ => Err(Error::Internal(
      format!("unexpected return type: {rowid_value:?}").into(),
    )),
  };
}
