use askama::Template;
use trailbase_schema::db::QualifiedNameEscaped;
use trailbase_schema::db::metadata::ColumnMetadata;
use trailbase_schema::json_schema::{FileUpload, FileUploads};
use trailbase_sqlite::Value;

use crate::records::error::RecordError;
use crate::records::expand::ExpandedTable;
use crate::schema_metadata::JsonColumnMetadata;

pub(crate) async fn run_select_query(
  conn: &trailbase_sqlite::Connection,
  table_name: &QualifiedNameEscaped,
  column_names: &[&str],
  pk_column: &str,
  pk_value: Value,
  expanded_tables: &[ExpandedTable<'_>],
) -> Result<Option<trailbase_sqlite::Row>, RecordError> {
  let sql = ReadRecordQueryTemplate {
    table_name,
    column_names,
    pk_column_name: pk_column,
    expanded_tables: expanded_tables,
  }
  .render()
  .map_err(|err| RecordError::Internal(err.into()))?;

  return Ok(conn.read_query_row(sql, [pk_value]).await?);
}

pub(crate) async fn run_get_file_query(
  conn: &trailbase_sqlite::Connection,
  table_name: &QualifiedNameEscaped,
  file_column_meta: &ColumnMetadata,
  pk_column: &str,
  pk_value: Value,
) -> Result<FileUpload, RecordError> {
  return match &file_column_meta.json {
    Some(JsonColumnMetadata::SchemaName(name)) if name == "std.FileUpload" => {
      let column_name = &file_column_meta.column.name;

      let Some(row) = conn
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

      return Ok(file_upload);
    }
    _ => Err(RecordError::BadRequest("Not a file")),
  };
}

pub(crate) async fn run_get_files_query(
  conn: &trailbase_sqlite::Connection,
  table_name: &QualifiedNameEscaped,
  file_column_meta: &ColumnMetadata,
  pk_column: &str,
  pk_value: Value,
) -> Result<FileUploads, RecordError> {
  return match &file_column_meta.json {
    Some(JsonColumnMetadata::SchemaName(name)) if name == "std.FileUploads" => {
      let column_name = &file_column_meta.column.name;

      let Some(row) = conn
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
    Some(JsonColumnMetadata::SchemaName(name)) if name == "std.FileUpload" => {
      return Ok(FileUploads(vec![
        run_get_file_query(conn, table_name, file_column_meta, pk_column, pk_value).await?,
      ]));
    }
    _ => Err(RecordError::BadRequest("Not a files list")),
  };
}

#[derive(Template)]
#[template(escape = "none", path = "read_record_query.sql")]
struct ReadRecordQueryTemplate<'a> {
  table_name: &'a QualifiedNameEscaped,
  column_names: &'a [&'a str],
  pk_column_name: &'a str,
  expanded_tables: &'a [ExpandedTable<'a>],
}
