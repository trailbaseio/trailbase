pub mod get_api_json_schema;

use axum::extract::{Json, State};
use serde::Serialize;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;

#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
pub struct JsonSchema {
  pub name: String,
  // NOTE: ideally we'd return an js `Object` here, however tanstack-form goes bonkers with
  // excessive type evaluation depth. Maybe we shouldn't use tanstack-form for schemas?
  pub schema: String,
  pub builtin: bool,
}

#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct ListJsonSchemasResponse {
  schemas: Vec<JsonSchema>,
}

#[utoipa::path(
  get,
  path = "/schema",
  tag = "admin",
  responses(
    (status = 200, description = "Success", body = ListJsonSchemasResponse),
  )
)]
pub async fn list_schemas_handler(
  State(state): State<AppState>,
) -> Result<Json<ListJsonSchemasResponse>, Error> {
  let schemas = state
    .json_schema_registry()
    .read()
    .entries()
    .iter()
    .map(|(name, schema)| {
      return JsonSchema {
        name: (*name).clone(),
        schema: schema.schema.to_string(),
        builtin: schema.builtin,
      };
    })
    .collect();

  return Ok(Json(ListJsonSchemasResponse { schemas }));
}
