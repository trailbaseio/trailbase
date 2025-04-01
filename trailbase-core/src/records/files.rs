use axum::body::Body;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use object_store::ObjectStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::*;
use trailbase_sqlite::params;
use trailbase_sqlite::schema::{FileUpload, FileUploads};

use crate::app_state::AppState;
use crate::table_metadata::TableMetadata;

#[derive(Debug, Error)]
pub enum FileError {
  #[error("Storage error: {0}")]
  Storage(#[from] object_store::Error),
  #[error("IO error: {0}")]
  IO(#[from] std::io::Error),
  #[error("Json serialization error: {0}")]
  JsonSerialization(#[from] serde_json::Error),
  #[error("SQL error: {0}")]
  Sql(#[from] trailbase_sqlite::Error),
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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct FileDeletionsDb {
  id: i64,
  deleted: i64,
  attempts: i64,
  errors: Option<String>,
  table_name: String,
  record_rowid: i64,
  column_name: String,
  json: String,
}

pub(crate) async fn delete_pending_files(
  state: &AppState,
  metadata: &TableMetadata,
  rowid: i64,
) -> Result<(), FileError> {
  if metadata.file_upload_columns.is_empty() && metadata.file_uploads_columns.is_empty() {
    return Ok(());
  }

  let table_name = &metadata.schema.name;
  let rows: Vec<FileDeletionsDb> = state
    .conn()
    .query_values(
      "SELECT * FROM _file_deletions WHERE table_name = ?1 AND record_rowid = ?2",
      params!(table_name.to_string(), rowid),
    )
    .await?;

  if rows.is_empty() {
    return Ok(());
  }

  let mut errors: Vec<FileDeletionsDb> = vec![];
  let mut delete = async |row: &FileDeletionsDb, file: FileUpload| {
    if let Err(err) = delete_file(state.objectstore(), file).await {
      let mut pending_deletion = row.clone();
      pending_deletion.attempts += 1;
      pending_deletion.errors = Some(err.to_string());
      errors.push(pending_deletion);
    }
  };

  for pending_deletion in rows {
    let json = &pending_deletion.json;

    if let Ok(file) = serde_json::from_str::<FileUpload>(json) {
      delete(&pending_deletion, file).await;
    } else if let Ok(files) = serde_json::from_str::<FileUploads>(json) {
      for file in files.0 {
        delete(&pending_deletion, file).await;
      }
    } else {
      error!("Pending file deletion w/o parsable contents: {json}");
    }
  }

  // Add errors back to try again later.
  for error in errors {
    if let Err(err) = state
      .conn()
      .execute(
        r#"
      INSERT INTO _file_deletions
        (deleted, attempts, errors, table_name, record_row_id, column_name, json)
      VALUES
        (?1, ?2, ?3, ?4, ?5, ?6, ?7)
      "#,
        params!(
          error.deleted,
          error.attempts,
          error.errors,
          error.table_name,
          error.record_rowid,
          error.column_name,
          error.json,
        ),
      )
      .await
    {
      warn!("Failed to restore pending file: {err}");
    }
  }

  return Ok(());
}

async fn delete_file(store: &dyn ObjectStore, file: FileUpload) -> Result<(), object_store::Error> {
  return store
    .delete(&object_store::path::Path::from(file.path()))
    .await;
}
