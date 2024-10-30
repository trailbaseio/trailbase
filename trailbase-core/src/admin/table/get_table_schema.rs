use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::table_metadata::{build_json_schema, JsonSchemaMode};

pub async fn get_table_schema_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
) -> Result<Response, Error> {
  let Some(table_metadata) = state.table_metadata().get(&table_name) else {
    return Err(Error::Precondition(format!("Table {table_name} not found")));
  };

  // TOOD: Allow controlling the schema mode to generate different types for insert, select, and
  // update.
  let (_schema, json) = build_json_schema(
    table_metadata.name(),
    &*table_metadata,
    JsonSchemaMode::Insert,
  )?;

  let mut response = serde_json::to_string_pretty(&json)?.into_response();
  response.headers_mut().insert(
    header::CONTENT_DISPOSITION,
    header::HeaderValue::from_static("attachment"),
  );
  return Ok(response);
}
