use axum::{
  Json,
  extract::{Path, State},
  http::StatusCode,
  response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use trailbase_schema::{QualifiedName, QualifiedNameEscaped};
use trailbase_sqlvalue::SqlValue;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::write_queries::run_delete_query;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeleteRowRequest {
  primary_key_column: String,
  /// The primary key value.
  value: SqlValue,
}

pub async fn delete_row_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Json(request): Json<DeleteRowRequest>,
) -> Result<Response, Error> {
  delete_row(
    &state,
    &QualifiedName::parse(&table_name)?,
    &request.primary_key_column,
    request.value,
  )
  .await?;
  return Ok((StatusCode::OK, "deleted").into_response());
}

pub(crate) async fn delete_row(
  state: &AppState,
  table_name: &QualifiedName,
  pk_col: &str,
  pk_value: SqlValue,
) -> Result<(), Error> {
  let metadata = state.connection_metadata();
  let Some(table_metadata) = metadata.get_table(table_name) else {
    return Err(Error::Precondition(format!(
      "Table {table_name:?} not found"
    )));
  };

  let Some((_index, column)) = table_metadata.column_by_name(pk_col) else {
    return Err(Error::Precondition(format!("Missing column: {pk_col}")));
  };

  if !column.is_primary() {
    return Err(Error::Precondition(format!("Not a primary key: {pk_col}")));
  }

  run_delete_query(
    state.conn(),
    state.objectstore(),
    &QualifiedNameEscaped::from(&table_metadata.schema.name),
    pk_col,
    pk_value.try_into()?,
    table_metadata.json_metadata.has_file_columns(),
  )
  .await?;

  return Ok(());
}

#[derive(Debug, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct DeleteRowsRequest {
  /// Name of the primary key column we use to identify which rows to delete.
  primary_key_column: String,

  /// A list of primary keys (of any type since we're in row instead of RecordApi land)
  /// of rows that shall be deleted.
  values: Vec<SqlValue>,
}

pub async fn delete_rows_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Json(request): Json<DeleteRowsRequest>,
) -> Result<Response, Error> {
  if state.demo_mode() && table_name.starts_with("_") {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let table_name = QualifiedName::parse(&table_name)?;
  let DeleteRowsRequest {
    primary_key_column,
    values,
  } = request;

  for value in values {
    // NOTE: Abort on first error.
    delete_row(&state, &table_name, &primary_key_column, value).await?;
  }

  return Ok((StatusCode::OK, "deleted all").into_response());
}

#[cfg(test)]
mod tests {
  use axum::extract::{Json, Path, RawQuery, State};
  use serde::Deserialize;
  use trailbase_schema::sqlite::{Column, ColumnAffinityType, ColumnDataType, ColumnOption, Table};
  use trailbase_sqlvalue::Blob;
  use uuid::Uuid;

  use super::*;
  use crate::admin::rows::insert_row::{InsertRowRequest, insert_row_handler};
  use crate::admin::rows::list_rows::list_rows_handler;
  use crate::admin::rows::update_row::{UpdateRowRequest, update_row_handler};
  use crate::admin::table::{CreateTableRequest, create_table_handler};
  use crate::app_state::*;
  use crate::util::uuid_to_b64;

  // TODO: This full-lifecycle test should probably live outside the scope of delete_row.
  #[tokio::test]
  async fn test_insert_update_delete_rows() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    #[derive(Deserialize)]
    struct TestTable {
      myid: Vec<u8>,
      col0: Option<String>,
    }

    let table_name = "test_table".to_string();
    let pk_col = "myid".to_string();

    let _ = create_table_handler(
      State(state.clone()),
      Json(CreateTableRequest {
        schema: Table {
          name: QualifiedName::parse(&table_name).unwrap(),
          strict: false,
          columns: vec![
            Column {
              name: pk_col.clone(),
              type_name: "BLOB".to_string(),
              data_type: ColumnDataType::Blob,
              affinity_type: ColumnAffinityType::Blob,
              options: vec![
                ColumnOption::Unique {
                  is_primary: true,
                  conflict_clause: None,
                },
                ColumnOption::Check(format!("(is_uuid_v7({pk_col}))")),
                ColumnOption::Default("(uuid_v7())".to_string()),
              ],
            },
            Column {
              name: "col0".to_string(),
              type_name: "text".to_string(),
              data_type: ColumnDataType::Text,
              affinity_type: ColumnAffinityType::Text,
              options: vec![],
            },
          ],
          foreign_keys: vec![],
          unique: vec![],
          checks: vec![],
          virtual_table: false,
          temporary: false,
        },
        dry_run: Some(false),
      }),
    )
    .await
    .unwrap();

    let insert = async |value: &str| {
      let Json(response) = insert_row_handler(
        State(state.clone()),
        Path(table_name.clone()),
        Json(InsertRowRequest {
          row: indexmap::IndexMap::from([("col0".to_string(), SqlValue::Text(value.to_string()))]),
        }),
      )
      .await
      .unwrap();

      return state
        .conn()
        .read_query_value::<TestTable>(
          format!("SELECT * FROM {table_name} WHERE _rowid_ = ?1"),
          trailbase_sqlite::params!(response.row_id),
        )
        .await
        .unwrap();
    };

    let id0 = {
      let row = insert("row0").await.unwrap();
      assert_eq!(row.col0.unwrap(), "row0");
      Uuid::from_slice(&row.myid).unwrap()
    };
    let id1 = {
      let row = insert("row1").await.unwrap();
      assert_eq!(row.col0.unwrap(), "row1");
      Uuid::from_slice(&row.myid).unwrap()
    };

    let count = || async {
      conn
        .read_query_row_f(format!("SELECT COUNT(*) FROM '{table_name}'"), (), |row| {
          row.get::<_, i64>(0)
        })
        .await
        .unwrap()
        .unwrap()
    };

    assert_eq!(count().await, 2);

    let updated_value = "row0 updated";
    update_row_handler(
      State(state.clone()),
      Path(table_name.clone()),
      Json(UpdateRowRequest {
        primary_key_column: pk_col.clone(),
        primary_key_value: SqlValue::Blob(Blob::Base64UrlSafe(uuid_to_b64(&id0))),
        row: indexmap::IndexMap::from([(
          "col0".to_string(),
          SqlValue::Text(updated_value.to_string()),
        )]),
      }),
    )
    .await
    .unwrap();

    let listing = list_rows_handler(
      State(state.clone()),
      Path(table_name.clone()),
      RawQuery(Some(format!("filter[{pk_col}]={}", uuid_to_b64(&id0)))),
    )
    .await
    .unwrap();

    assert_eq!(listing.rows.len(), 1, "Listing: {listing:?}");
    assert_eq!(
      listing.rows[0][1],
      SqlValue::Text(updated_value.to_string())
    );

    let delete = |id: uuid::Uuid| {
      delete_row_handler(
        State(state.clone()),
        Path(table_name.clone()),
        Json(DeleteRowRequest {
          primary_key_column: pk_col.clone(),
          value: SqlValue::Blob(Blob::Base64UrlSafe(uuid_to_b64(&id))),
        }),
      )
    };

    delete(id0).await.unwrap();
    delete(id1).await.unwrap();

    assert_eq!(count().await, 0);
  }
}
