use askama::Template;
use std::sync::Arc;
use trailbase_schema::sqlite::Column;
use trailbase_schema::{FileUpload, FileUploads, QualifiedNameEscaped};
use trailbase_sqlite::Value;

use crate::AppState;
use crate::records::error::RecordError;
use crate::records::expand::ExpandedTable;
use crate::schema_metadata::{JsonColumnMetadata, TableMetadata};

pub(crate) async fn run_select_query(
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

pub(crate) struct ExpandedSelectQueryResult {
  pub root: trailbase_sqlite::Row,
  pub foreign_rows: Vec<(Arc<TableMetadata>, trailbase_sqlite::Row)>,
}

pub(crate) async fn run_expanded_select_query(
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

pub(crate) async fn run_get_file_query(
  state: &AppState,
  table_name: &QualifiedNameEscaped,
  file_column: &Column,
  json_metadata: &JsonColumnMetadata,
  pk_column: &str,
  pk_value: Value,
) -> Result<FileUpload, RecordError> {
  return match &json_metadata {
    JsonColumnMetadata::SchemaName(name) if name == "std.FileUpload" => {
      let column_name = &file_column.name;

      let Some(row) = state
        .conn()
        .read_query_row(
          format!(r#"SELECT "{column_name}" FROM {table_name} WHERE "{pk_column}" = $1"#),
          [pk_value],
        )
        .await
        .map_err(|err| RecordError::Internal(err.into()))?
      else {
        return Err(RecordError::RecordNotFound);
      };

      let json: String = row
        .get(0)
        .map_err(|err| RecordError::Internal(err.into()))?;
      let file_upload: FileUpload =
        serde_json::from_str(&json).map_err(|err| RecordError::Internal(err.into()))?;
      Ok(file_upload)
    }
    _ => Err(RecordError::BadRequest("Not a file")),
  };
}

pub(crate) async fn run_get_files_query(
  state: &AppState,
  table_name: &QualifiedNameEscaped,
  file_column: &Column,
  json_metadata: &JsonColumnMetadata,
  pk_column: &str,
  pk_value: Value,
) -> Result<FileUploads, RecordError> {
  return match &json_metadata {
    JsonColumnMetadata::SchemaName(name) if name == "std.FileUploads" => {
      let column_name = &file_column.name;

      let Some(row) = state
        .conn()
        .read_query_row(
          format!(r#"SELECT "{column_name}" FROM {table_name} WHERE "{pk_column}" = $1"#),
          [pk_value],
        )
        .await
        .map_err(|err| RecordError::Internal(err.into()))?
      else {
        return Err(RecordError::RecordNotFound);
      };

      let contents: String = row
        .get(0)
        .map_err(|err| RecordError::Internal(err.into()))?;
      let file_uploads: FileUploads =
        serde_json::from_str(&contents).map_err(|err| RecordError::Internal(err.into()))?;
      Ok(file_uploads)
    }
    JsonColumnMetadata::SchemaName(name) if name == "std.FileUpload" => {
      return Ok(FileUploads(vec![
        run_get_file_query(
          state,
          table_name,
          file_column,
          json_metadata,
          pk_column,
          pk_value,
        )
        .await?,
      ]));
    }
    _ => Err(RecordError::BadRequest("Not a files list")),
  };
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
