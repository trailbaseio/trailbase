use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::table_metadata::{build_json_schema, JsonSchemaMode};

#[derive(Clone, Debug, Deserialize)]
pub struct GetTableSchemaParams {
  mode: Option<JsonSchemaMode>,
}

pub async fn get_table_schema_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Query(query): Query<GetTableSchemaParams>,
) -> Result<Response, Error> {
  let Some(table_metadata) = state.table_metadata().get(&table_name) else {
    return Err(Error::Precondition(format!("Table {table_name} not found")));
  };

  let (_schema, json) = build_json_schema(
    table_metadata.name(),
    &table_metadata.schema.columns,
    query.mode.unwrap_or(JsonSchemaMode::Insert),
  )?;

  let mut response = serde_json::to_string_pretty(&json)?.into_response();
  response.headers_mut().insert(
    header::CONTENT_DISPOSITION,
    header::HeaderValue::from_static("attachment"),
  );
  return Ok(response);
}
