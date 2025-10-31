mod get_api_json_schema;

pub(super) use get_api_json_schema::get_api_json_schema_handler;

use axum::extract::{Json, State};
use serde::Serialize;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;

#[derive(Debug, Serialize, TS)]
pub struct JsonSchema {
  pub name: String,
  // NOTE: ideally we'd return an js `Object` here, however tanstack-form goes bonkers with
  // excessive type evaluation depth. Maybe we shouldn't use tanstack-form for schemas?
  pub schema: String,
  pub builtin: bool,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ListJsonSchemasResponse {
  schemas: Vec<JsonSchema>,
}

pub async fn list_schemas_handler(
  State(_state): State<AppState>,
) -> Result<Json<ListJsonSchemasResponse>, Error> {
  let registry = trailbase_extension::jsonschema::json_schema_registry_snapshot();

  let schemas = registry
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
