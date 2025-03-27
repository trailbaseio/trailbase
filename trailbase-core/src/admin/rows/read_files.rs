use axum::{
  extract::{Path, Query, State},
  response::Response,
};
use serde::Deserialize;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::files::read_file_into_response;
use crate::records::params::simple_json_value_to_param;
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
  let Some(table_metadata) = state.table_metadata().get(&table_name) else {
    return Err(Error::Precondition(format!("Table {table_name} not found")));
  };
  let pk_col = &request.pk_column;

  let Some((col, _col_meta)) = table_metadata.column_by_name(pk_col) else {
    return Err(Error::Precondition(format!("Missing column: {pk_col}")));
  };

  if !col.is_primary() {
    return Err(Error::Precondition(format!("Not a primary key: {pk_col}")));
  }

  let Some(file_col_metadata) = table_metadata.column_by_name(&request.file_column_name) else {
    return Err(Error::Precondition(format!("Missing column: {pk_col}")));
  };

  let pk_value = simple_json_value_to_param(col.data_type, request.pk_value)?;

  return if let Some(file_index) = request.file_index {
    let mut file_uploads = GetFilesQueryBuilder::run(
      &state,
      &table_name,
      file_col_metadata,
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
      &table_name,
      file_col_metadata,
      &request.pk_column,
      pk_value,
    )
    .await?;

    Ok(read_file_into_response(&state, file_upload).await?)
  };
}
