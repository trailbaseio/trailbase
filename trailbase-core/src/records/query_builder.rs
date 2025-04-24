use askama::Template;
use itertools::Itertools;
use log::*;
use std::sync::Arc;
use trailbase_schema::sqlite::{Column, ColumnOption};
use trailbase_schema::{FileUpload, FileUploads};
use trailbase_sqlite::{NamedParams, Params as _, Value};

use crate::AppState;
use crate::config::proto::ConflictResolutionStrategy;
use crate::records::error::RecordError;
use crate::records::files::{FileManager, delete_pending_files};
use crate::records::params::{FileMetadataContents, Params};
use crate::table_metadata::{JsonColumnMetadata, TableMetadata, TableMetadataCache};

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
  #[error("Precondition error: {0}")]
  Precondition(&'static str),
  #[error("FromSql error: {0}")]
  FromSql(#[from] rusqlite::types::FromSqlError),
  #[error("Tokio Rusqlite error: {0}")]
  TokioRusqlite(#[from] trailbase_sqlite::Error),
  #[error("Json serialization error: {0}")]
  JsonSerialization(#[from] serde_json::Error),
  #[error("ObjectStore error: {0}")]
  Storage(#[from] object_store::Error),
  #[error("File error: {0}")]
  File(#[from] crate::records::files::FileError),
  #[error("Not found")]
  NotFound,
  #[error("Internal: {0}")]
  Internal(Box<dyn std::error::Error + Send + Sync>),
}

pub(crate) struct ExpandedTable {
  pub metadata: Arc<TableMetadata>,
  pub local_column_name: String,
  pub num_columns: usize,

  pub foreign_table_name: String,
  pub foreign_column_name: String,
}

pub(crate) fn expand_tables<T: AsRef<str>>(
  table_metadata: &TableMetadataCache,
  table_name: &str,
  expand: &[T],
) -> Result<Vec<ExpandedTable>, RecordError> {
  let Some(root_table) = table_metadata.get(table_name) else {
    return Err(RecordError::ApiRequiresTable);
  };

  let mut expanded_tables = Vec::<ExpandedTable>::with_capacity(expand.len());

  for col_name in expand {
    let col_name = col_name.as_ref();
    if col_name.is_empty() {
      continue;
    }
    let Some((_index, column)) = root_table.column_by_name(col_name) else {
      return Err(RecordError::ApiRequiresTable);
    };

    // FIXME: This only expand FKs expressed as column constraints missing table constraints.
    let Some(ColumnOption::ForeignKey {
      foreign_table: foreign_table_name,
      referred_columns: _,
      ..
    }) = column
      .options
      .iter()
      .find_or_first(|o| matches!(o, ColumnOption::ForeignKey { .. }))
    else {
      return Err(RecordError::ApiRequiresTable);
    };

    let Some(foreign_table) = table_metadata.get(foreign_table_name) else {
      return Err(RecordError::ApiRequiresTable);
    };

    let Some(foreign_pk_column_idx) = foreign_table.record_pk_column else {
      return Err(RecordError::ApiRequiresTable);
    };

    let foreign_pk_column = &foreign_table.schema.columns[foreign_pk_column_idx].name;

    // TODO: Check that `referred_columns` and foreign_pk_column are the same. It's already
    // validated as part of config validation.

    let num_columns = foreign_table.schema.columns.len();
    let foreign_table_name = foreign_table_name.to_string();
    let foreign_column_name = foreign_pk_column.to_string();

    expanded_tables.push(ExpandedTable {
      metadata: foreign_table,
      local_column_name: col_name.to_string(),
      num_columns,
      foreign_table_name,
      foreign_column_name,
    });
  }

  return Ok(expanded_tables);
}

#[derive(Template)]
#[template(escape = "none", path = "read_record_query_expanded.sql")]
struct ReadRecordExpandedQueryTemplate<'a> {
  table_name: &'a str,
  column_names: &'a [&'a str],
  pk_column_name: &'a str,
  expanded_tables: &'a [ExpandedTable],
}

#[derive(Template)]
#[template(escape = "none", path = "read_record_query.sql")]
struct ReadRecordQueryTemplate<'a> {
  table_name: &'a str,
  column_names: &'a [&'a str],
  pk_column_name: &'a str,
}

pub(crate) struct SelectQueryBuilder;

impl SelectQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &str,
    column_names: &[&str],
    pk_column: &str,
    pk_value: Value,
  ) -> Result<Option<trailbase_sqlite::Row>, RecordError> {
    let sql = ReadRecordQueryTemplate {
      table_name,
      column_names,
      pk_column_name: pk_column,
    }
    .render()
    .map_err(|err| RecordError::Internal(err.into()))?;

    return Ok(state.conn().read_query_row(sql, [pk_value]).await?);
  }

  pub(crate) async fn run_expanded(
    state: &AppState,
    table_name: &str,
    column_names: &[&str],
    pk_column: &str,
    pk_value: Value,
    expand: &[&str],
  ) -> Result<Vec<(Arc<TableMetadata>, trailbase_sqlite::Row)>, RecordError> {
    let table_metadata = state.table_metadata();

    let Some(main_table) = table_metadata.get(table_name) else {
      return Err(RecordError::ApiRequiresTable);
    };

    let expanded_tables = expand_tables(table_metadata, table_name, expand)?;
    let sql = ReadRecordExpandedQueryTemplate {
      table_name,
      column_names,
      pk_column_name: pk_column,
      expanded_tables: &expanded_tables,
    }
    .render()
    .map_err(|err| RecordError::Internal(err.into()))?;

    let Some(mut row) = state.conn().read_query_row(sql, [pk_value]).await? else {
      return Ok(vec![]);
    };

    let mut result = Vec::with_capacity(expanded_tables.len() + 1);
    let mut curr = row.split_off(column_names.len());
    result.push((main_table, row));

    for expanded in expanded_tables {
      let next = curr.split_off(expanded.num_columns);
      result.push((expanded.metadata, curr));
      curr = next;
    }

    return Ok(result);
  }
}

pub(crate) struct GetFileQueryBuilder;

impl GetFileQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &str,
    file_column: &Column,
    json_metadata: &JsonColumnMetadata,
    pk_column: &str,
    pk_value: Value,
  ) -> Result<FileUpload, QueryError> {
    return match &json_metadata {
      JsonColumnMetadata::SchemaName(name) if name == "std.FileUpload" => {
        let column_name = &file_column.name;

        let Some(row) = state
          .conn()
          .read_query_row(
            format!(r#"SELECT "{column_name}" FROM "{table_name}" WHERE "{pk_column}" = $1"#),
            [pk_value],
          )
          .await?
        else {
          return Err(QueryError::NotFound);
        };

        let json: String = row.get(0)?;
        let file_upload: FileUpload = serde_json::from_str(&json)?;
        Ok(file_upload)
      }
      _ => Err(QueryError::Precondition("Not a file")),
    };
  }
}

pub(crate) struct GetFilesQueryBuilder;

impl GetFilesQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &str,
    file_column: &Column,
    json_metadata: &JsonColumnMetadata,
    pk_column: &str,
    pk_value: Value,
  ) -> Result<FileUploads, QueryError> {
    return match &json_metadata {
      JsonColumnMetadata::SchemaName(name) if name == "std.FileUploads" => {
        let column_name = &file_column.name;

        let Some(row) = state
          .conn()
          .read_query_row(
            format!(r#"SELECT "{column_name}" FROM "{table_name}" WHERE "{pk_column}" = $1"#),
            [pk_value],
          )
          .await?
        else {
          return Err(QueryError::NotFound);
        };

        let contents: String = row.get(0)?;
        let file_uploads: FileUploads = serde_json::from_str(&contents)?;
        Ok(file_uploads)
      }
      _ => Err(QueryError::Precondition("Not a files list")),
    };
  }
}

#[derive(Template)]
#[template(escape = "none", path = "create_record_query.sql")]
struct CreateRecordQueryTemplate<'a> {
  table_name: &'a str,
  conflict_clause: &'a str,
  column_names: &'a [String],
  returning: &'a [&'a str],
}

pub(crate) struct InsertQueryBuilder;

impl InsertQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &str,
    conflict_resolution: Option<ConflictResolutionStrategy>,
    return_column_name: &str,
    has_file_columns: bool,
    params: Params,
  ) -> Result<rusqlite::types::Value, QueryError> {
    let (query, named_params, files) = Self::build_insert_query(
      table_name,
      params,
      conflict_resolution,
      Some(return_column_name),
    )?;

    // We're storing any files to the object store first to make sure the DB entry is valid right
    // after commit and not racily pointing to soon-to-be-written files.
    let mut file_manager = if files.is_empty() {
      FileManager::empty()
    } else {
      FileManager::write(state, files).await?
    };

    let (rowid, return_value): (i64, rusqlite::types::Value) = state
      .conn()
      .query_row_f(query, named_params, |row| -> Result<_, rusqlite::Error> {
        return Ok((row.get(0)?, row.get(1)?));
      })
      .await?
      .ok_or_else(|| trailbase_sqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows))?;

    // Successful write, do not cleanup written files.
    file_manager.release();

    if Some(ConflictResolutionStrategy::Replace) == conflict_resolution && has_file_columns {
      delete_pending_files(state, table_name, rowid).await?;
    }

    return Ok(return_value);
  }

  pub(crate) async fn run_bulk(
    state: &AppState,
    table_name: &str,
    conflict_resolution: Option<ConflictResolutionStrategy>,
    return_column_name: &str,
    has_file_columns: bool,
    params_list: Vec<Params>,
  ) -> Result<Vec<rusqlite::types::Value>, QueryError> {
    let mut all_files: FileMetadataContents = vec![];
    let mut query_and_params: Vec<(String, NamedParams)> = vec![];

    for params in params_list {
      let (query, named_params, mut files) = Self::build_insert_query(
        table_name,
        params,
        conflict_resolution,
        Some(return_column_name),
      )?;

      all_files.append(&mut files);
      query_and_params.push((query, named_params));
    }

    // We're storing any files to the object store first to make sure the DB entry is valid right
    // after commit and not racily pointing to soon-to-be-written files.
    let mut file_manager = if all_files.is_empty() {
      FileManager::empty()
    } else {
      FileManager::write(state, all_files).await?
    };

    let result: Vec<(i64, rusqlite::types::Value)> = state
      .conn()
      .call(move |conn| {
        let mut rows = Vec::<(i64, rusqlite::types::Value)>::with_capacity(query_and_params.len());

        let tx = conn.transaction()?;

        for (query, named_params) in query_and_params {
          let mut stmt = tx.prepare_cached(&query)?;
          named_params.bind(&mut stmt)?;
          let mut result = stmt.raw_query();

          match result.next()? {
            Some(row) => rows.push((row.get(0)?, row.get(1)?)),
            _ => {
              return Err(rusqlite::Error::QueryReturnedNoRows.into());
            }
          };
        }

        tx.commit()?;

        return Ok(rows);
      })
      .await?;

    // Successful write, do not cleanup written files.
    file_manager.release();

    if Some(ConflictResolutionStrategy::Replace) == conflict_resolution && has_file_columns {
      for (rowid, _) in &result {
        delete_pending_files(state, table_name, *rowid).await?;
      }
    }

    return Ok(result.into_iter().map(|(_rowid, v)| v).collect());
  }

  fn build_insert_query(
    table_name: &str,
    params: Params,
    conflict_resolution: Option<ConflictResolutionStrategy>,
    return_column_name: Option<&str>,
  ) -> Result<(String, NamedParams, FileMetadataContents), QueryError> {
    let conflict_clause = match conflict_resolution {
      Some(ConflictResolutionStrategy::Abort) => "OR ABORT",
      Some(ConflictResolutionStrategy::Rollback) => "OR ROLLBACK",
      Some(ConflictResolutionStrategy::Fail) => "OR FAIL",
      Some(ConflictResolutionStrategy::Ignore) => "OR IGNORE",
      Some(ConflictResolutionStrategy::Replace) => "OR REPLACE",
      _ => "",
    };

    let returning: &[&str] = if let Some(return_column_name) = return_column_name {
      &["_rowid_", return_column_name]
    } else {
      &["_rowid_"]
    };

    let query = CreateRecordQueryTemplate {
      table_name,
      conflict_clause,
      column_names: &params.column_names,
      returning,
    }
    .render()
    .map_err(|err| QueryError::Internal(err.into()))?;

    return Ok((query, params.named_params, params.files));
  }
}

#[derive(Template)]
#[template(escape = "none", path = "update_record_query.sql")]
struct UpdateRecordQueryTemplate<'a> {
  table_name: &'a str,
  column_names: &'a [String],
  pk_column_name: &'a str,
  returning: Option<&'a str>,
}

pub(crate) struct UpdateQueryBuilder;

impl UpdateQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &str,
    pk_column: &str,
    has_file_columns: bool,
    mut params: Params,
  ) -> Result<(), QueryError> {
    if params.column_names.len() < 2 {
      // Only the primary key. Nothing to do.
      assert!(params.column_names.is_empty() || params.column_names[0] == pk_column);
      return Ok(());
    }

    // We're storing any files to the object store first to make sure the DB entry is valid right
    // after commit and not racily pointing to soon-to-be-written files.
    let files = std::mem::take(&mut params.files);
    let mut file_manager = if files.is_empty() {
      FileManager::empty()
    } else {
      FileManager::write(state, files).await?
    };

    let query = UpdateRecordQueryTemplate {
      table_name,
      column_names: &params.column_names,
      pk_column_name: pk_column,
      returning: Some("_rowid_"),
    }
    .render()
    .map_err(|err| QueryError::Internal(err.into()))?;

    let rowid: Option<i64> = state
      .conn()
      .query_row_f(query, params.named_params, |row| row.get(0))
      .await?;

    // Successful write, do not cleanup written files.
    file_manager.release();

    if has_file_columns {
      if let Some(rowid) = rowid {
        delete_pending_files(state, table_name, rowid).await?;
      }
    }

    return Ok(());
  }
}

pub(crate) struct DeleteQueryBuilder;

impl DeleteQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &str,
    pk_column: &str,
    pk_value: Value,
    has_file_columns: bool,
  ) -> Result<i64, QueryError> {
    let rowid: i64 = state
      .conn()
      .query_row_f(
        format!(r#"DELETE FROM "{table_name}" WHERE "{pk_column}" = $1 RETURNING _rowid_"#),
        [pk_value],
        |row| row.get(0),
      )
      .await?
      .ok_or_else(|| QueryError::NotFound)?;

    if has_file_columns {
      delete_pending_files(state, table_name, rowid).await?;
    }

    return Ok(rowid);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use trailbase_schema::sqlite::sqlite3_parse_into_statement;

  fn sanitize_template(template: &str) {
    assert!(sqlite3_parse_into_statement(template).is_ok(), "{template}");
    assert!(!template.contains("\n"), "{template}");
    assert!(!template.contains("   "), "{template}");
  }

  #[test]
  fn test_create_record_template() {
    {
      let query = CreateRecordQueryTemplate {
        table_name: "table",
        conflict_clause: "OR ABORT",
        column_names: &["index".to_string(), "trigger".to_string()],
        returning: &["index"],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = CreateRecordQueryTemplate {
        table_name: "table",
        conflict_clause: "",
        column_names: &[],
        returning: &["*"],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = CreateRecordQueryTemplate {
        table_name: "table",
        conflict_clause: "",
        column_names: &["index".to_string()],
        returning: &[],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }
  }
}
