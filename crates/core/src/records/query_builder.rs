use askama::Template;
use itertools::Itertools;
use log::*;
use std::sync::Arc;
use thiserror::Error;
use trailbase_schema::sqlite::{Column, ColumnOption};
use trailbase_schema::{FileUpload, FileUploads, QualifiedName, QualifiedNameEscaped};
use trailbase_sqlite::{NamedParams, Params as _, Value};

use crate::AppState;
use crate::config::proto::ConflictResolutionStrategy;
use crate::records::error::RecordError;
use crate::records::files::{FileError, FileManager, delete_files_marked_for_deletion};
use crate::records::params::{FileMetadataContents, Params};
use crate::schema_metadata::{JsonColumnMetadata, SchemaMetadataCache, TableMetadata};

#[derive(Debug, Error)]
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
  File(#[from] FileError),
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

pub(crate) fn expand_tables<'a, 'b, T: AsRef<str>>(
  schema_metadata: &SchemaMetadataCache,
  database_schema: &Option<String>,
  root_column_by_name: impl Fn(&'a str) -> Option<&'b Column>,
  expand: &'a [T],
) -> Result<Vec<ExpandedTable>, RecordError> {
  let mut expanded_tables = Vec::<ExpandedTable>::with_capacity(expand.len());

  for col_name in expand {
    let col_name = col_name.as_ref();
    if col_name.is_empty() {
      continue;
    }
    let Some(column) = root_column_by_name(col_name) else {
      return Err(RecordError::Internal("Missing column".into()));
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
      return Err(RecordError::Internal("not a foreign key".into()));
    };

    let Some(foreign_table) = schema_metadata.get_table(&QualifiedName {
      name: foreign_table_name.clone(),
      database_schema: database_schema.clone(),
    }) else {
      return Err(RecordError::ApiRequiresTable);
    };

    let Some(foreign_pk_column_idx) = foreign_table.record_pk_column else {
      return Err(RecordError::Internal("invalid PK".into()));
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
  table_name: &'a QualifiedNameEscaped,
  column_names: &'a [&'a str],
  pk_column_name: &'a str,
  expanded_tables: &'a [ExpandedTable],
}

#[derive(Template)]
#[template(escape = "none", path = "read_record_query.sql")]
struct ReadRecordQueryTemplate<'a> {
  table_name: &'a QualifiedNameEscaped,
  column_names: &'a [&'a str],
  pk_column_name: &'a str,
}

pub(crate) struct SelectQueryBuilder;

pub(crate) struct ExpandedSelectQueryResult {
  pub(crate) root: trailbase_sqlite::Row,
  pub(crate) foreign_rows: Vec<(Arc<TableMetadata>, trailbase_sqlite::Row)>,
}

impl SelectQueryBuilder {
  pub(crate) async fn run(
    conn: &trailbase_sqlite::Connection,
    table_name: &QualifiedNameEscaped,
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

    return Ok(conn.read_query_row(sql, [pk_value]).await?);
  }

  pub(crate) async fn run_expanded(
    conn: &trailbase_sqlite::Connection,
    table_name: &QualifiedNameEscaped,
    column_names: &[&str],
    pk_column: &str,
    pk_value: Value,
    expanded_tables: &[ExpandedTable],
  ) -> Result<Option<ExpandedSelectQueryResult>, RecordError> {
    let sql = ReadRecordExpandedQueryTemplate {
      table_name,
      column_names,
      pk_column_name: pk_column,
      expanded_tables,
    }
    .render()
    .map_err(|err| RecordError::Internal(err.into()))?;

    let Some(mut row) = conn.read_query_row(sql, [pk_value]).await? else {
      return Ok(None);
    };

    let mut foreign_rows: Vec<(Arc<TableMetadata>, trailbase_sqlite::Row)> =
      Vec::with_capacity(expanded_tables.len());

    let mut curr = row.split_off(column_names.len());
    for expanded_table in expanded_tables {
      let next = curr.split_off(expanded_table.num_columns);
      foreign_rows.push((expanded_table.metadata.clone(), curr));
      curr = next;
    }

    return Ok(Some(ExpandedSelectQueryResult {
      root: row,
      foreign_rows,
    }));
  }
}

pub(crate) struct GetFileQueryBuilder;

impl GetFileQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &QualifiedNameEscaped,
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
            format!(r#"SELECT "{column_name}" FROM {table_name} WHERE "{pk_column}" = $1"#),
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
    table_name: &QualifiedNameEscaped,
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
            format!(r#"SELECT "{column_name}" FROM {table_name} WHERE "{pk_column}" = $1"#),
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
  table_name: &'a QualifiedNameEscaped,
  conflict_clause: &'a str,
  column_names: &'a [String],
  returning: &'a [&'a str],
}

pub(crate) struct InsertQueryBuilder;

impl InsertQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &QualifiedNameEscaped,
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
      delete_files_marked_for_deletion(state, table_name, &[rowid]).await?;
    }

    return Ok(return_value);
  }

  pub(crate) async fn run_bulk(
    state: &AppState,
    table_name: &QualifiedNameEscaped,
    conflict_resolution: Option<ConflictResolutionStrategy>,
    return_column_name: &str,
    has_file_columns: bool,
    params_list: Vec<Params>,
  ) -> Result<Vec<rusqlite::types::Value>, QueryError> {
    let mut all_files: FileMetadataContents = vec![];
    let query_and_params = params_list
      .into_iter()
      .map(|params| -> Result<_, QueryError> {
        let (query, named_params, mut files) = Self::build_insert_query(
          table_name,
          params,
          conflict_resolution,
          Some(return_column_name),
        )?;

        all_files.append(&mut files);
        return Ok((query, named_params));
      })
      .collect::<Result<Vec<_>, _>>()?;

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

    if result.is_empty() {
      return Ok(vec![]);
    }

    // Successful write, do not cleanup written files.
    file_manager.release();

    if Some(ConflictResolutionStrategy::Replace) == conflict_resolution && has_file_columns {
      let (rowids, values): (Vec<i64>, Vec<_>) = result.into_iter().unzip();
      delete_files_marked_for_deletion(state, table_name, &rowids).await?;
      return Ok(values);
    }

    return Ok(result.into_iter().map(|(_rowid, v)| v).collect());
  }

  #[inline]
  fn build_insert_query(
    table_name: &QualifiedNameEscaped,
    params: Params,
    conflict_resolution: Option<ConflictResolutionStrategy>,
    return_column_name: Option<&str>,
  ) -> Result<(String, NamedParams, FileMetadataContents), QueryError> {
    let Params::Insert {
      named_params,
      files,
      column_names,
      column_indexes: _,
    } = params
    else {
      return Err(QueryError::Internal("not an insert".into()));
    };

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
      column_names: &column_names,
      returning,
    }
    .render()
    .map_err(|err| QueryError::Internal(err.into()))?;

    return Ok((query, named_params, files));
  }
}

#[derive(Template)]
#[template(escape = "none", path = "update_record_query.sql")]
struct UpdateRecordQueryTemplate<'a> {
  table_name: &'a QualifiedNameEscaped,
  column_names: &'a [String],
  pk_column_name: &'a str,
  returning: Option<&'a str>,
}

pub(crate) struct UpdateQueryBuilder;

impl UpdateQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &QualifiedNameEscaped,
    has_file_columns: bool,
    params: Params,
  ) -> Result<(), QueryError> {
    let Params::Update {
      named_params,
      files,
      column_names,
      column_indexes: _,
      pk_column_name,
    } = params
    else {
      return Err(QueryError::Internal("not an update".into()));
    };
    if column_names.is_empty() {
      // Nothing to do.
      return Ok(());
    }

    // We're storing any files to the object store first to make sure the DB entry is valid right
    // after commit and not racily pointing to soon-to-be-written files.
    let mut file_manager = if files.is_empty() {
      FileManager::empty()
    } else {
      FileManager::write(state, files).await?
    };

    let query = UpdateRecordQueryTemplate {
      table_name,
      column_names: &column_names,
      pk_column_name: &pk_column_name,
      returning: Some("_rowid_"),
    }
    .render()
    .map_err(|err| QueryError::Internal(err.into()))?;

    let rowid: Option<i64> = state
      .conn()
      .query_row_f(query, named_params, |row| row.get(0))
      .await?;

    // Successful write, do not cleanup written files.
    file_manager.release();

    if has_file_columns {
      if let Some(rowid) = rowid {
        delete_files_marked_for_deletion(state, table_name, &[rowid]).await?;
      }
    }

    return Ok(());
  }
}

pub(crate) struct DeleteQueryBuilder;

impl DeleteQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &QualifiedNameEscaped,
    pk_column: &str,
    pk_value: Value,
    has_file_columns: bool,
  ) -> Result<i64, QueryError> {
    let rowid: i64 = state
      .conn()
      .query_row_f(
        format!(r#"DELETE FROM {table_name} WHERE "{pk_column}" = $1 RETURNING _rowid_"#),
        [pk_value],
        |row| row.get(0),
      )
      .await?
      .ok_or_else(|| QueryError::NotFound)?;

    if has_file_columns {
      delete_files_marked_for_deletion(state, table_name, &[rowid]).await?;
    }

    return Ok(rowid);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use trailbase_schema::parse::parse_into_statement;
  use trailbase_schema::sqlite::QualifiedName;

  fn sanitize_template(template: &str) {
    assert!(parse_into_statement(template).is_ok(), "{template}");
    assert!(!template.contains("\n\n"), "{template}");
    assert!(!template.contains("   "), "{template}");
  }

  #[test]
  fn test_create_record_template() {
    {
      let query = CreateRecordQueryTemplate {
        table_name: &QualifiedName::parse("table").unwrap().into(),
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
        table_name: &QualifiedName {
          name: "table".to_string(),
          database_schema: Some("db".to_string()),
        }
        .into(),
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
        table_name: &QualifiedName::parse("table").unwrap().into(),
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
