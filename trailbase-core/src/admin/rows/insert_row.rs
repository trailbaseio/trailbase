use axum::extract::{Path, State};
use axum::Json;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::json_to_sql::{InsertQueryBuilder, Params};
use crate::records::sql_to_json::row_to_json_array;

type Row = Vec<serde_json::Value>;

pub async fn insert_row_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Json(request): Json<serde_json::Value>,
) -> Result<Json<Row>, Error> {
  let row = insert_row(&state, table_name, request).await?;
  return Ok(Json(row));
}

pub async fn insert_row(
  state: &AppState,
  table_name: String,
  value: serde_json::Value,
) -> Result<Row, Error> {
  let Some(table_metadata) = state.table_metadata().get(&table_name) else {
    return Err(Error::Precondition(format!("Table {table_name} not found")));
  };

  let row = InsertQueryBuilder::run(
    state,
    Params::from(&table_metadata, value, None)?,
    None,
    Some("*"),
  )
  .await?;

  return Ok(row_to_json_array(row)?);
}
