use axum::{
  extract::{Path, State},
  http::StatusCode,
  response::{IntoResponse, Response},
  Json,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::json_to_sql::simple_json_value_to_param;
use crate::records::json_to_sql::DeleteQueryBuilder;

#[derive(Debug, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct DeleteRowRequest {
  primary_key_column: String,

  /// The primary key (of any type since we're in row instead of RecordApi land) of rows that
  /// shall be deleted.
  #[ts(type = "Object")]
  value: serde_json::Value,
}

pub async fn delete_row_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Json(request): Json<DeleteRowRequest>,
) -> Result<Response, Error> {
  delete_row(
    &state,
    table_name,
    &request.primary_key_column,
    request.value,
  )
  .await?;
  return Ok((StatusCode::OK, "deleted").into_response());
}

async fn delete_row(
  state: &AppState,
  table_name: String,
  pk_col: &str,
  value: serde_json::Value,
) -> Result<(), Error> {
  let Some(table_metadata) = state.table_metadata().get(&table_name) else {
    return Err(Error::Precondition(format!("Table {table_name} not found")));
  };

  let Some((column, _col_meta)) = table_metadata.column_by_name(pk_col) else {
    return Err(Error::Precondition(format!("Missing column: {pk_col}")));
  };

  if !column.is_primary() {
    return Err(Error::Precondition(format!("Not a primary key: {pk_col}")));
  }

  DeleteQueryBuilder::run(
    state,
    &table_metadata,
    pk_col,
    simple_json_value_to_param(column.data_type, value)?,
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
  #[ts(type = "Object[]")]
  values: Vec<serde_json::Value>,
}

pub async fn delete_rows_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Json(request): Json<DeleteRowsRequest>,
) -> Result<Response, Error> {
  let DeleteRowsRequest {
    primary_key_column,
    values,
  } = request;

  for value in values {
    delete_row(&state, table_name.clone(), &primary_key_column, value).await?;
  }

  return Ok((StatusCode::OK, "deleted all").into_response());
}

#[cfg(test)]
mod tests {
  use axum::extract::{Json, Path, RawQuery, State};

  use super::*;
  use crate::admin::rows::insert_row::insert_row;
  use crate::admin::rows::list_rows::list_rows_handler;
  use crate::admin::rows::update_row::{update_row_handler, UpdateRowRequest};
  use crate::admin::table::{create_table_handler, CreateTableRequest};
  use crate::app_state::*;
  use crate::records::test_utils::json_row_from_value;
  use crate::schema::{Column, ColumnDataType, ColumnOption, Table};
  use crate::util::{b64_to_uuid, uuid_to_b64};

  // TODO: This full-lifecycle test should probably live outside the scope of delete_row.
  #[tokio::test]
  async fn test_insert_update_delete_rows() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    let table_name = "test_table".to_string();
    let pk_col = "myid".to_string();
    let _ = create_table_handler(
      State(state.clone()),
      Json(CreateTableRequest {
        schema: Table {
          name: table_name.clone(),
          strict: false,
          columns: vec![
            Column {
              name: pk_col.clone(),
              data_type: ColumnDataType::Blob,
              options: vec![
                ColumnOption::Unique { is_primary: true },
                ColumnOption::Check(format!("(is_uuid_v7({pk_col}))")),
                ColumnOption::Default("(uuid_v7())".to_string()),
              ],
            },
            Column {
              name: "col0".to_string(),
              data_type: ColumnDataType::Text,
              options: vec![],
            },
          ],
          foreign_keys: vec![],
          unique: vec![],
          virtual_table: false,
          temporary: false,
        },
        dry_run: Some(false),
      }),
    )
    .await
    .unwrap();

    let insert = |value: &str| {
      insert_row(
        &state,
        table_name.clone(),
        json_row_from_value(serde_json::json!({
          "col0": value,
        }))
        .unwrap(),
      )
    };

    let get_id = |row: Vec<serde_json::Value>| {
      return match &row[0] {
        serde_json::Value::String(str) => b64_to_uuid(str).unwrap(),
        x => {
          panic!("unexpected type: {x:?}");
        }
      };
    };

    let id0 = {
      let row = insert("row0").await.unwrap();
      assert_eq!(&row[1], "row0");
      get_id(row)
    };
    let id1 = {
      let row = insert("row1").await.unwrap();
      assert_eq!(&row[1], "row1");
      get_id(row)
    };

    let count = async || {
      conn
        .query_row(&format!("SELECT COUNT(*) FROM '{table_name}'"), ())
        .await
        .unwrap()
        .unwrap()
        .get::<i64>(0)
        .unwrap()
    };

    assert_eq!(count().await, 2);

    let updated_value = "row0 updated";
    update_row_handler(
      State(state.clone()),
      Path(table_name.clone()),
      Json(UpdateRowRequest {
        primary_key_column: pk_col.clone(),
        primary_key_value: serde_json::Value::String(uuid_to_b64(&id0)),
        row: json_row_from_value(serde_json::json!({
          "col0": updated_value.to_string(),
        }))
        .unwrap(),
      }),
    )
    .await
    .unwrap();

    let listing = list_rows_handler(
      State(state.clone()),
      Path(table_name.clone()),
      RawQuery(Some(format!("{pk_col}={}", uuid_to_b64(&id0)))),
    )
    .await
    .unwrap();

    assert_eq!(listing.rows.len(), 1, "Listing: {listing:?}");
    assert_eq!(
      listing.rows[0][1],
      serde_json::Value::String(updated_value.to_string())
    );

    let delete = |id: uuid::Uuid| {
      delete_row_handler(
        State(state.clone()),
        Path(table_name.clone()),
        Json(DeleteRowRequest {
          primary_key_column: pk_col.clone(),
          value: serde_json::Value::String(uuid_to_b64(&id)),
        }),
      )
    };

    delete(id0).await.unwrap();
    delete(id1).await.unwrap();

    assert_eq!(count().await, 0);
  }
}
