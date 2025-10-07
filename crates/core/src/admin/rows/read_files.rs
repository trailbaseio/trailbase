use axum::{
  extract::{Path, Query, State},
  response::Response,
};
use serde::Deserialize;
use trailbase_schema::json::flat_json_to_value;
use trailbase_schema::{FileUploads, QualifiedName};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::files::read_file_into_response;
use crate::records::read_queries::run_get_files_query;

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ReadFilesQuery {
  pk_column: String,

  /// The primary key (of any type since we're in row instead of RecordAPI land) of rows that
  /// shall be deleted.
  #[ts(type = "Object")]
  pk_value: serde_json::Value,

  file_column_name: String,
  file_name: Option<String>,
}

pub async fn read_files_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Query(query): Query<ReadFilesQuery>,
) -> Result<Response, Error> {
  let table_name = QualifiedName::parse(&table_name)?;
  let Some(schema_metadata) = state.schema_metadata().get_table(&table_name) else {
    return Err(Error::Precondition(format!(
      "Table {table_name:?} not found"
    )));
  };
  let pk_col = &query.pk_column;

  let Some((_index, col)) = schema_metadata.column_by_name(pk_col) else {
    return Err(Error::Precondition(format!("Missing column: {pk_col}")));
  };

  if !col.is_primary() {
    return Err(Error::Precondition(format!("Not a primary key: {pk_col}")));
  }

  let Some((index, file_col_metadata)) = schema_metadata.column_by_name(&query.file_column_name)
  else {
    return Err(Error::Precondition(format!(
      "Missing column: {}",
      query.file_column_name
    )));
  };
  let Some(file_col_json_metadata) = schema_metadata.json_metadata.columns[index].as_ref() else {
    return Err(Error::Precondition(format!(
      "Not a JSON column: {}",
      query.file_column_name
    )));
  };

  let pk_value = flat_json_to_value(col.data_type, query.pk_value, true)?;

  let FileUploads(mut file_uploads) = run_get_files_query(
    &state,
    &table_name.into(),
    file_col_metadata,
    file_col_json_metadata,
    &query.pk_column,
    pk_value,
  )
  .await?;

  if file_uploads.is_empty() {
    return Err(Error::Precondition("Empty list of files".to_string()));
  }

  return if let Some(filename) = query.file_name {
    let Some(file) = file_uploads.into_iter().find(|f| f.filename() == filename) else {
      return Err(Error::Precondition(format!("File '{filename}' not found")));
    };

    Ok(read_file_into_response(&state, file).await?)
  } else {
    Ok(read_file_into_response(&state, file_uploads.remove(0)).await?)
  };
}
