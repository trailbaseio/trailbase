use axum::extract::{Path, Query, State};
use axum::http::{self, HeaderName, HeaderValue};
use axum::response::{AppendHeaders, IntoResponse, Response};
use serde::Deserialize;
use trailbase_schema::json_schema::JsonSchemaMode;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::json_schema::build_api_json_schema;

/// Determines the action for which to generate the JSON schema.
///
/// For example, inserts may fall back to default values making such parameters optional.
#[derive(Clone, Debug, Deserialize, utoipa::ToSchema)]
pub enum SchemaMode {
  /// Insert mode.
  Insert,
  /// Read/Select mode.
  Select,
  /// Update mode.
  Update,
}

#[derive(Clone, Debug, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct GetTableSchemaParams {
  mode: Option<SchemaMode>,
}

#[utoipa::path(
  get,
  path = "/schema/{record_api_name}/schema.json",
  tag = "admin",
  params(GetTableSchemaParams),
  responses(
    (status = 200, description = "Success"),
    (status = 412, description = "API does not exist"),
  )
)]
pub async fn get_api_json_schema_handler(
  State(state): State<AppState>,
  Path(record_api_name): Path<String>,
  Query(query): Query<GetTableSchemaParams>,
) -> Result<Response, Error> {
  let Some(api) = state.lookup_record_api(&record_api_name) else {
    return Err(Error::Precondition(format!(
      "API {record_api_name} does not exist"
    )));
  };

  let json = build_api_json_schema(
    &state,
    &api,
    query.mode.map(|m| match m {
      SchemaMode::Insert => JsonSchemaMode::Insert,
      SchemaMode::Update => JsonSchemaMode::Update,
      SchemaMode::Select => JsonSchemaMode::Select,
    }),
  )
  .map_err(|err| Error::Internal(err.into()))?;

  return Ok(
    (
      AppendHeaders(CONTENT_DISPOSITION_HEADER),
      serde_json::to_string_pretty(&json)?,
    )
      .into_response(),
  );
}

// Open the "Save as" dialog in the browser.
const CONTENT_DISPOSITION_HEADER: [(HeaderName, HeaderValue); 1] = [(
  http::header::CONTENT_DISPOSITION,
  HeaderValue::from_static("attachment"),
)];
