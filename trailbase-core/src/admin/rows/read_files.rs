use axum::{
  extract::{Path, Query, State},
  response::Response,
};
use serde::Deserialize;
use trailbase_schema::QualifiedName;
use trailbase_schema::json::flat_json_to_value;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::files::read_file_into_response;
use crate::records::query_builder::{GetFileQueryBuilder, GetFilesQueryBuilder};

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ReadFilesRequest {
  pk_column: String,

  /// The primary key (of any type since we're in row instead of RecordAPI land) of rows that
  /// shall be deleted.
  #[ts(type = "Object")]
  pk_value: serde_json::Value,

  file_column_name: String,
  file_index: Option<usize>,
}

pub async fn read_files_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Query(request): Query<ReadFilesRequest>,
) -> Result<Response, Error> {
  let table_name = QualifiedName::parse(&table_name)?;
  let Some(schema_metadata) = state.schema_metadata().get_table(&table_name) else {
    return Err(Error::Precondition(format!(
      "Table {table_name:?} not found"
    )));
  };
  let pk_col = &request.pk_column;

  let Some((_index, col)) = schema_metadata.column_by_name(pk_col) else {
    return Err(Error::Precondition(format!("Missing column: {pk_col}")));
  };

  if !col.is_primary() {
    return Err(Error::Precondition(format!("Not a primary key: {pk_col}")));
  }

  let Some((index, file_col_metadata)) = schema_metadata.column_by_name(&request.file_column_name)
  else {
    return Err(Error::Precondition(format!(
      "Missing column: {}",
      request.file_column_name
    )));
  };
  let Some(file_col_json_metadata) = schema_metadata.json_metadata.columns[index].as_ref() else {
    return Err(Error::Precondition(format!(
      "Not a JSON column: {}",
      request.file_column_name
    )));
  };

  let pk_value = flat_json_to_value(col.data_type, request.pk_value, true)?;

  return if let Some(file_index) = request.file_index {
    let mut file_uploads = GetFilesQueryBuilder::run(
      &state,
      &table_name.into(),
      file_col_metadata,
      file_col_json_metadata,
      &request.pk_column,
      pk_value,
    )
    .await?;

    if file_index >= file_uploads.0.len() {
      return Err(Error::Precondition(format!("Out of bounds: {file_index}")));
    }

    Ok(read_file_into_response(&state, file_uploads.0.remove(file_index)).await?)
  } else {
    let file_upload = GetFileQueryBuilder::run(
      &state,
      &table_name.into(),
      file_col_metadata,
      file_col_json_metadata,
      &request.pk_column,
      pk_value,
    )
    .await?;

    Ok(read_file_into_response(&state, file_upload).await?)
  };
}
