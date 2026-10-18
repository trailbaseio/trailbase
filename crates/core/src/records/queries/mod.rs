use object_store::ObjectStore;
use std::sync::Arc;
use trailbase_schema::db::QualifiedNameEscaped;
use trailbase_schema::db::metadata::ColumnMetadata;
use trailbase_sqlite::{Connection, Value};

use crate::config::proto;
use crate::records::error::RecordError;
use crate::records::files::{FileManager, delete_files_marked_for_deletion};
use crate::records::params::{FileMetadataContents, InsertParams, UpdateParams};

mod delete;
mod insert;
mod read;
mod update;

pub use delete::DeleteRecordApiQuery;
pub use insert::InsertOrReplaceRecordApiQuery;
pub use read::ReadRecordApiQuery;
pub use update::UpdateRecordApiQuery;

pub struct WriteQueryResult {
  /// Row-id (SQLite) or ctid (Postgres) of affected record. Always present.
  pub rowid: i64,
  /// Primary key value. Only present on inserts.
  pub pk_value: Option<Value>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_insert_or_replace_query(
  conn: &Connection,
  objectstore: &Arc<dyn ObjectStore>,
  table_name: &QualifiedNameEscaped,
  column_metadata: &[ColumnMetadata],
  conflict_resolution: proto::ConflictResolutionStrategy,
  return_column_name: &str,
  params: InsertParams,
  files: FileMetadataContents,
) -> Result<Value, RecordError> {
  let query = insert::InsertOrReplaceQuery::new(
    conn.connection_type(),
    table_name,
    column_metadata,
    return_column_name,
    conflict_resolution,
    params,
  )?;

  // We're storing any files to the object store first to make sure the DB entry is valid right
  // after commit and not racily pointing to soon-to-be-written files.
  let file_manager = if files.is_empty() {
    None
  } else {
    Some(FileManager::write(objectstore, files).await?)
  };

  let WriteQueryResult { rowid, pk_value } = conn
    .call_writer(move |mut conn| query.apply(&mut conn))
    .await?;

  let Some(return_value) = pk_value else {
    return Err(RecordError::Internal("missing pk".into()));
  };

  // Successful write, do not cleanup written files unless old ones were overridden.
  if let Some(mut file_manager) = file_manager {
    file_manager.release();

    if conflict_resolution == proto::ConflictResolutionStrategy::Replace {
      delete_files_marked_for_deletion(conn, objectstore, table_name, &[rowid])
        .await
        .map_err(|err| RecordError::Internal(err.into()))?;
    }
  }

  return Ok(return_value);
}

pub(crate) async fn run_update_query(
  conn: &Connection,
  objectstore: &Arc<dyn ObjectStore>,
  table_name: &QualifiedNameEscaped,
  params: UpdateParams,
  files: FileMetadataContents,
) -> Result<(), RecordError> {
  let query = update::UpdateQuery::new(conn.connection_type(), table_name, params)?;

  // We're storing any files to the object store first to make sure the DB entry is valid right
  // after commit and not racily pointing to soon-to-be-written files.
  let file_manager = if files.is_empty() {
    None
  } else {
    Some(FileManager::write(objectstore, files).await?)
  };

  let WriteQueryResult { rowid, pk_value: _ } = conn
    .call_writer(move |mut conn| query.apply(&mut conn))
    .await?;

  // Successful write, do not cleanup written files.
  if let Some(mut file_manager) = file_manager {
    file_manager.release();
    delete_files_marked_for_deletion(conn, objectstore, table_name, &[rowid])
      .await
      .map_err(|err| RecordError::Internal(err.into()))?;
  }

  return Ok(());
}

pub(crate) async fn run_delete_query(
  conn: &Connection,
  objectstore: &Arc<dyn ObjectStore>,
  table_name: &QualifiedNameEscaped,
  pk_column: &str,
  pk_value: Value,
  has_file_columns: bool,
) -> Result<i64, RecordError> {
  let query = delete::DeleteQuery::new(conn.connection_type(), table_name, pk_column, pk_value)?;

  let WriteQueryResult { rowid, pk_value: _ } = conn
    .call_writer(move |mut conn| query.apply(&mut conn))
    .await?;

  if has_file_columns {
    delete_files_marked_for_deletion(conn, objectstore, table_name, &[rowid])
      .await
      .map_err(|err| RecordError::Internal(err.into()))?;
  }

  return Ok(rowid);
}
