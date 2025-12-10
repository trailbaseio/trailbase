use axum::body::Body;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use itertools::Itertools;
use log::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use trailbase_schema::{FileUpload, FileUploads, QualifiedNameEscaped};
use trailbase_sqlite::params;

use crate::app_state::{AppState, ObjectStore};
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
  let result = store
    .get(&object_store::path::Path::from(
      file_upload.objectstore_id(),
    ))
    .await?;

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

/// Deletes files already marked for deletion (by trigger) for the given rowid.
///
/// NOTE: We're specific on the record/rowid, rather than deleting all pending files, to avoid
/// blocking a response on unrelated delations.
/// QUESTION: Should we delete eagerly at all? We could just do this periodically.
pub(crate) async fn delete_files_marked_for_deletion(
  conn: &trailbase_sqlite::Connection,
  store: &Arc<ObjectStore>,
  table_name: &QualifiedNameEscaped,
  rowids: &[i64],
) -> Result<(), FileError> {
  let rows: Vec<FileDeletionsDb> = match rowids.len() {
    0 => {
      return Ok(());
    }
    1 => {
        conn
        .write_query_values(
          "DELETE FROM main._file_deletions WHERE table_name = ?1 AND record_rowid = ?2 RETURNING *",
          trailbase_sqlite::params!(table_name.to_string(), rowids[0]),
        )
        .await?
    }
    _ => conn
      .read_query_values(
        format!(
          "DELETE FROM main._file_deletions WHERE table_name = {table_name} AND record_rowid IN ({ids}) RETURNING *",
            ids = rowids.iter().join(", "),
        ),
        (),
      )
      .await?,
  };

  // Question: Should we do this opportunistically like during updates?
  if !rows.is_empty() {
    delete_pending_files_impl(conn, store, rows).await?;
  }

  return Ok(());
}

pub(crate) async fn delete_pending_files_impl(
  conn: &trailbase_sqlite::Connection,
  store: &Arc<ObjectStore>,
  pending_deletions: Vec<FileDeletionsDb>,
) -> Result<(), FileError> {
  const ATTEMPTS_LIMIT: i64 = 10;
  if pending_deletions.is_empty() {
    return Ok(());
  }

  let mut errors: Vec<FileDeletionsDb> = vec![];
  let mut delete = async |row: &FileDeletionsDb, file: FileUpload| {
    let result = store
      .delete(&object_store::path::Path::from(file.objectstore_id()))
      .await;

    match result {
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
          warn!("Abandoning deletion of {file:?} after {ATTEMPTS_LIMIT} attempts: {err}");
        }
      }
      Ok(_) => {}
    };
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

pub(crate) struct FileManager {
  cleanup: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl FileManager {
  pub(crate) async fn write(
    store: &Arc<ObjectStore>,
    files: FileMetadataContents,
  ) -> Result<Self, object_store::Error> {
    let mut written_files = Vec::<FileUpload>::with_capacity(files.len());
    for (metadata, contents) in files {
      // TODO: We could write files in parallel.
      let path = object_store::path::Path::from(metadata.objectstore_id());

      let mut writer = store.put_multipart(&path).await?;
      writer.put_part(contents.into()).await?;
      writer.complete().await?;

      written_files.push(metadata);
    }

    let cleanup: Option<Box<dyn FnOnce() + Send + Sync>> = if written_files.is_empty() {
      None
    } else {
      let store = store.clone();
      Some(Box::new(move || {
        tokio::spawn(async move {
          for file in written_files {
            let path = object_store::path::Path::from(file.objectstore_id());
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
