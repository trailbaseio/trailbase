use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use trailbase_schema::{QualifiedName, QualifiedNameEscaped};
use trailbase_sqlvalue::SqlValue;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::connection::ConnectionEntry;
use crate::records::params::Params;
use crate::records::write_queries::run_update_query;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateRowRequest {
  pub primary_key_column: String,
  pub primary_key_value: SqlValue,

  /// Row data, which is expected to be a map from column name to value.
  ///
  /// Note that the row is represented as a map to allow selective cells as opposed to
  /// Vec<SqlValue>. Absence is different from setting a column to NULL.
  pub row: indexmap::IndexMap<String, SqlValue>,
}

pub async fn update_row_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Json(request): Json<UpdateRowRequest>,
) -> Result<(), Error> {
  if state.demo_mode() && table_name.starts_with("_") {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let table_name = QualifiedName::parse(&table_name)?;
  let ConnectionEntry {
    connection: conn,
    metadata,
  } = state
    .connection_manager()
    .get_entry_for_qn(&table_name)
    .await?;

  let Some(table_metadata) = metadata.get_table(&table_name) else {
    return Err(Error::Precondition(format!(
      "Table {table_name:?} not found"
    )));
  };

  let pk_col = &request.primary_key_column;
  let Some(meta) = table_metadata.column_by_name(pk_col) else {
    return Err(Error::Precondition(format!("Missing column: {pk_col}")));
  };

  if let Some(pk_index) = table_metadata.record_pk_column
    && meta.index != pk_index
  {
    return Err(Error::Precondition(format!("Pk column mismatch: {pk_col}")));
  }

  let column = &meta.column;
  if !column.is_primary() {
    return Err(Error::Precondition(format!("Not a primary key: {pk_col}")));
  }

  run_update_query(
    &conn,
    state.objectstore(),
    &QualifiedNameEscaped::new(&table_metadata.schema.name),
    Params::for_admin_update(
      table_metadata,
      state.json_schema_registry().clone(),
      request.row,
      pk_col.clone(),
      request.primary_key_value,
    )?,
  )
  .await?;

  return Ok(());
}

#[cfg(test)]
mod tests {
  use axum::extract::Query;
  use std::collections::HashMap;

  use axum::extract::RawQuery;
  use trailbase_schema::{FileUpload, FileUploadData, FileUploadInput};

  use super::*;
  use crate::admin::rows::list_rows::{ListRowsResponse, list_rows_handler};
  use crate::admin::rows::read_files::{ReadFilesQuery, read_files_handler};
  use crate::app_state::test_state;
  use crate::test_utils::*;

  #[tokio::test]
  async fn file_upload_admin_apis_test() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    const TABLE_NAME: &str = "test_table";

    // Make sure as admin APIs, this also works for non-STRICT tables.
    conn
      .execute_batch(format!(
        "
        CREATE TABLE {TABLE_NAME} (
          id           {serial} PRIMARY KEY,
          data         TEXT NOT NULL,
          file         {json} CHECK(jsonschema('std.FileUpload', file)),
          files        {json} CHECK(jsonschema('std.FileUploads', files))
        );

        INSERT INTO {TABLE_NAME} (id, data) VALUES (1, 'foo');
        ",
        serial = serial_column(conn),
        json = json_column(conn),
      ))
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    let bytes0: Vec<u8> = vec![0, 1, 2, 3, 4, 5];
    let file0 = FileUploadInput {
      name: Some("foo0".to_string()),
      filename: Some("bar0.png".to_string()),
      content_type: None,
      data: FileUploadData(bytes0.clone()),
    };

    update_row_handler(
      State(state.clone()),
      Path(TABLE_NAME.to_string()),
      Json(UpdateRowRequest {
        primary_key_column: "id".to_string(),
        primary_key_value: SqlValue::Integer(1),
        row: indexmap::IndexMap::from([
          ("data".to_string(), SqlValue::Text("updated".to_string())),
          (
            "file".to_string(),
            SqlValue::Text(serde_json::to_string(&file0).unwrap()),
          ),
        ]),
      }),
    )
    .await
    .unwrap();

    let Json(resp): Json<ListRowsResponse> = list_rows_handler(
      State(state.clone()),
      Path(TABLE_NAME.to_string()),
      RawQuery(None),
    )
    .await
    .unwrap();

    assert_eq!(1, resp.rows.len());
    assert_eq!(1, resp.total_row_count);
    assert_eq!(4, resp.columns.len());

    let obj = {
      let mut obj = HashMap::new();
      let row = &resp.rows[0];
      for i in 0..resp.columns.len() {
        obj.insert(resp.columns[i].name.clone(), row[i].clone());
      }
      obj
    };

    let SqlValue::Text(contents) = obj.get("file").unwrap() else {
      panic!("not text: {obj:?}");
    };

    let file_upload: FileUpload = serde_json::from_str(&contents).unwrap();
    assert_eq!(
      file_upload.original_filename(),
      Some("bar0.png"),
      "{file_upload:?}"
    );
    assert!(
      file_upload.filename().starts_with("bar0"),
      "{file_upload:?}"
    );

    let response = read_files_handler(
      State(state.clone()),
      Path(TABLE_NAME.to_string()),
      Query(ReadFilesQuery {
        pk_column: "id".to_string(),
        pk_value: "1".to_string(),
        file_column_name: "file".to_string(),
        file_name: None,
      }),
    )
    .await
    .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
      .await
      .unwrap();

    assert_eq!(bytes0, bytes);
  }
}
