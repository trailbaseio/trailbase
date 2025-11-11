use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use trailbase_schema::json_schema::JsonSchemaMode;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::json_schema::build_api_json_schema;

#[derive(Clone, Debug, Deserialize)]
pub struct GetTableSchemaParams {
  mode: Option<JsonSchemaMode>,
}

pub async fn get_api_json_schema_handler(
  State(state): State<AppState>,
  Path(record_api_name): Path<String>,
  Query(query): Query<GetTableSchemaParams>,
) -> Result<Response, Error> {
  let Some(api) = state.lookup_record_api(&record_api_name) else {
    return Err(Error::Precondition(format!(
      "API {record_api_name} not found"
    )));
  };

  let json =
    build_api_json_schema(&state, &api, query.mode).map_err(|err| Error::Internal(err.into()))?;

  let mut response = serde_json::to_string_pretty(&json)?.into_response();
  response.headers_mut().insert(
    header::CONTENT_DISPOSITION,
    header::HeaderValue::from_static("attachment"),
  );
  return Ok(response);
}
