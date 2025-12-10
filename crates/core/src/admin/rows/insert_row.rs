use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use trailbase_schema::{QualifiedName, QualifiedNameEscaped};
use trailbase_sqlvalue::SqlValue;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::params::Params;
use crate::records::write_queries::run_insert_query;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InsertRowRequest {
  /// Row data, which is expected to be a map from column name to value.
  pub row: indexmap::IndexMap<String, SqlValue>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InsertRowResponse {
  pub row_id: i64,
}

pub async fn insert_row_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Json(request): Json<InsertRowRequest>,
) -> Result<Json<InsertRowResponse>, Error> {
  let table_name = QualifiedName::parse(&table_name)?;
  let conn = super::build_connection(&state, &table_name)?;
  let metadata = super::build_connection_metadata(&state, &conn, &table_name).await?;
  let Some(table_metadata) = metadata.get_table(&table_name) else {
    return Err(Error::Precondition(format!(
      "Table {table_name:?} not found"
    )));
  };

  let rowid_value = run_insert_query(
    &conn,
    state.objectstore(),
    &QualifiedNameEscaped::new(&table_metadata.schema.name),
    None,
    "_rowid_",
    // NOTE: We "fancy" parse JSON string values, since the UI currently ships everything as a
    // string. We could consider pushing some more type-awareness into the ui.
    Params::for_admin_insert(table_metadata, request.row)?,
  )
  .await?;

  return match rowid_value {
    rusqlite::types::Value::Integer(rowid) => Ok(Json(InsertRowResponse { row_id: rowid })),
    _ => Err(Error::Internal(
      format!("unexpected return type: {rowid_value:?}").into(),
    )),
  };
}
