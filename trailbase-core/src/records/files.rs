use axum::body::Body;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use log::*;
use object_store::ObjectStore;
use thiserror::Error;
use trailbase_sqlite::schema::{FileUpload, FileUploads};

use crate::app_state::AppState;
use crate::table_metadata::{JsonColumnMetadata, TableOrViewMetadata};

#[derive(Debug, Error)]
pub enum FileError {
  #[error("Storage error: {0}")]
  Storage(#[from] object_store::Error),
  #[error("IO error: {0}")]
  IO(#[from] std::io::Error),
  #[error("Json serialization error: {0}")]
  JsonSerialization(#[from] serde_json::Error),
}

pub(crate) async fn read_file_into_response(
  state: &AppState,
  file_upload: FileUpload,
) -> Result<Response, FileError> {
  let store = state.objectstore();
  let path = object_store::path::Path::from(file_upload.path());
  let result = store.get(&path).await?;

  let headers = || {
    return [
      (
        header::CONTENT_TYPE,
        file_upload.content_type().map_or_else(
          || "text/plain; charset=utf-8".to_string(),
          |c| c.to_string(),
        ),
      ),
      (header::CONTENT_DISPOSITION, "attachment".to_string()),
    ];
  };

  return match result.payload {
    object_store::GetResultPayload::File(_file, path) => {
      let contents = tokio::fs::read(path).await?;
      Ok((headers(), Body::from(contents)).into_response())
    }
    object_store::GetResultPayload::Stream(stream) => {
      Ok((headers(), Body::from_stream(stream)).into_response())
    }
  };
}

pub(crate) async fn delete_files_in_row(
  state: &AppState,
  metadata: &(dyn TableOrViewMetadata + Send + Sync),
  row: trailbase_sqlite::Row,
) -> Result<(), FileError> {
  for i in 0..row.column_count() {
    let Some(col_name) = row.column_name(i) else {
      warn!("Missing name: {i}");
      continue;
    };
    let Some((_column, column_metadata)) = metadata.column_by_name(col_name) else {
      warn!("Missing column: {col_name}");
      continue;
    };

    if let Some(json) = &column_metadata.json {
      let store = state.objectstore();
      match json {
        JsonColumnMetadata::SchemaName(name) if name == "std.FileUpload" => {
          if let Ok(json) = row.get::<String>(i) {
            let file: FileUpload = serde_json::from_str(&json)?;
            delete_file(store, file).await?;
          }
        }
        JsonColumnMetadata::SchemaName(name) if name == "std.FileUploads" => {
          if let Ok(json) = row.get::<String>(i) {
            let file_uploads: FileUploads = serde_json::from_str(&json)?;
            for file in file_uploads.0 {
              delete_file(store, file).await?;
            }
          }
        }
        _ => {}
      }
    }
  }

  return Ok(());
}

// async fn maybe_delete_files_in_column(
//   state: &AppState,
//   column: &ColumnMetadata,
// ) -> Result<(), object_store::Error> {
//   return Ok(());
// }

async fn delete_file(store: &dyn ObjectStore, file: FileUpload) -> Result<(), object_store::Error> {
  return store
    .delete(&object_store::path::Path::from(file.path()))
    .await;
}
