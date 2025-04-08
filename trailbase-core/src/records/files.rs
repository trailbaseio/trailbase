use axum::body::Body;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use log::*;
use object_store::ObjectStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use trailbase_schema::{FileUpload, FileUploads};
use trailbase_sqlite::params;

use crate::app_state::AppState;
use crate::records::params::FileMetadataContents;

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
pub(crate) struct FileDeletionsDb {
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
  table_name: &str,
  rowid: i64,
) -> Result<(), FileError> {
  let rows: Vec<FileDeletionsDb> = state
    .conn()
    .read_query_values(
      "SELECT * FROM _file_deletions WHERE table_name = ?1 AND record_rowid = ?2",
      params!(table_name.to_string(), rowid),
    )
    .await?;

  delete_pending_files_impl(state.conn(), state.objectstore(), rows).await?;

  return Ok(());
}

pub(crate) async fn delete_pending_files_impl(
  conn: &trailbase_sqlite::Connection,
  store: &dyn ObjectStore,
  pending_deletions: Vec<FileDeletionsDb>,
) -> Result<(), FileError> {
  const ATTEMPTS_LIMIT: i64 = 10;
  if pending_deletions.is_empty() {
    return Ok(());
  }

  let mut errors: Vec<FileDeletionsDb> = vec![];
  let mut delete =
    async |row: &FileDeletionsDb, file: FileUpload| match delete_file(store, &file).await {
      Err(object_store::Error::NotFound { .. }) | Err(object_store::Error::InvalidPath { .. }) => {
        info!("Dropping further deletion attempts for invalid file: {file:?}");
      }
      Err(err) => {
        if row.attempts < ATTEMPTS_LIMIT {
          let mut pending_deletion = row.clone();
          pending_deletion.attempts += 1;
          pending_deletion.errors = Some(err.to_string());
          errors.push(pending_deletion);
        } else {
          info!("Abandoning deletion of {file:?} after {ATTEMPTS_LIMIT} failed attemps: {err}");
        }
      }
      Ok(_) => {}
    };

  for pending_deletion in pending_deletions {
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
    if let Err(err) = conn
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

async fn delete_file(
  store: &dyn ObjectStore,
  file: &FileUpload,
) -> Result<(), object_store::Error> {
  return store
    .delete(&object_store::path::Path::from(file.path()))
    .await;
}

pub(crate) struct FileManager {
  cleanup: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl FileManager {
  pub(crate) fn empty() -> Self {
    return Self { cleanup: None };
  }

  pub(crate) async fn write(
    state: &AppState,
    files: FileMetadataContents,
  ) -> Result<Self, object_store::Error> {
    let mut written_files = Vec::<FileUpload>::with_capacity(files.len());
    for (metadata, contents) in files {
      // TODO: We could write files in parallel.
      write_file(state.objectstore(), &metadata, contents).await?;
      written_files.push(metadata);
    }

    let cleanup: Option<Box<dyn FnOnce() + Send + Sync>> = if written_files.is_empty() {
      None
    } else {
      let state = state.clone();
      Some(Box::new(move || {
        tokio::spawn(async move {
          let store = state.objectstore();
          for file in written_files {
            let path = object_store::path::Path::from(file.path());
            if let Err(err) = store.delete(&path).await {
              warn!("Failed to cleanup just written file: {err}");
            }
          }
        });
      }))
    };

    return Ok(Self { cleanup });
  }

  pub(crate) fn release(&mut self) {
    self.cleanup = None;
  }
}

impl Drop for FileManager {
  fn drop(&mut self) {
    if let Some(f) = std::mem::take(&mut self.cleanup) {
      f();
    }
  }
}

async fn write_file(
  store: &dyn ObjectStore,
  metadata: &FileUpload,
  data: Vec<u8>,
) -> Result<(), object_store::Error> {
  let path = object_store::path::Path::from(metadata.path());

  let mut writer = store.put_multipart(&path).await?;
  writer.put_part(data.into()).await?;
  writer.complete().await?;

  return Ok(());
}
