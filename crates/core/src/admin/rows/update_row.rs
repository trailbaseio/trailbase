use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use trailbase_schema::{QualifiedName, QualifiedNameEscaped};
use trailbase_sqlvalue::SqlValue;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::connection::ConnectionEntry;
use crate::records::params::Params;
use crate::records::write_queries::run_update_query;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateRowRequest {
  pub primary_key_column: String,
  pub primary_key_value: SqlValue,

  /// Row data, which is expected to be a map from column name to value.
  ///
  /// Note that the row is represented as a map to allow selective cells as opposed to
  /// Vec<SqlValue>. Absence is different from setting a column to NULL.
  pub row: indexmap::IndexMap<String, SqlValue>,
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
  let ConnectionEntry {
    connection: conn,
    metadata,
  } = state.connection_manager().get_entry_for_qn(&table_name)?;

  let Some(table_metadata) = metadata.get_table(&table_name) else {
    return Err(Error::Precondition(format!(
      "Table {table_name:?} not found"
    )));
  };

  let pk_col = &request.primary_key_column;
  let Some(meta) = table_metadata.column_by_name(pk_col) else {
    return Err(Error::Precondition(format!("Missing column: {pk_col}")));
  };

  if let Some(pk_index) = table_metadata.record_pk_column
    && meta.index != pk_index
  {
    return Err(Error::Precondition(format!("Pk column mismatch: {pk_col}")));
  }

  let column = &meta.column;
  if !column.is_primary() {
    return Err(Error::Precondition(format!("Not a primary key: {pk_col}")));
  }

  run_update_query(
    &conn,
    state.objectstore(),
    &QualifiedNameEscaped::new(&table_metadata.schema.name),
    Params::for_admin_update(
      table_metadata,
      request.row,
      pk_col.clone(),
      request.primary_key_value,
    )?,
  )
  .await?;

  return Ok(());
}
