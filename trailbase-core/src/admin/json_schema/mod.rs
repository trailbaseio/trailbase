mod get_api_json_schema;

pub(super) use get_api_json_schema::get_api_json_schema_handler;

use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use trailbase_schema::registry::{get_schemas, set_user_schema};

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

impl From<trailbase_schema::registry::Schema> for JsonSchema {
  fn from(value: trailbase_schema::registry::Schema) -> Self {
    return JsonSchema {
      name: value.name,
      schema: value.schema.to_string(),
      builtin: value.builtin,
    };
  }
}

pub async fn list_schemas_handler(
  State(_state): State<AppState>,
) -> Result<Json<ListJsonSchemasResponse>, Error> {
  let schemas = get_schemas();

  return Ok(Json(ListJsonSchemasResponse {
    schemas: schemas.into_iter().map(|s| s.into()).collect(),
  }));
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateJsonSchemaRequest {
  name: String,
  #[ts(type = "Object | undefined")]
  schema: Option<serde_json::Value>,
}

pub async fn update_schema_handler(
  State(state): State<AppState>,
  Json(request): Json<UpdateJsonSchemaRequest>,
) -> Result<Json<serde_json::Value>, Error> {
  // Update the schema in memory.
  let (name, schema) = (request.name, request.schema);
  set_user_schema(&name, schema.clone())?;

  // And if that succeeds update config.
  let mut config = state.get_config();
  if let Some(schema) = schema {
    // Add/update
    let mut found = false;
    for s in &mut config.schemas {
      if s.name.as_ref() == Some(&name) {
        s.schema = Some(schema.to_string());
        found = true;
      }
    }

    if !found {
      config.schemas.push(crate::config::proto::JsonSchemaConfig {
        name: Some(name.clone()),
        schema: Some(schema.to_string()),
      })
    }
  } else {
    // Remove
    config.schemas = config
      .schemas
      .into_iter()
      .filter_map(|s| {
        if s.name.as_ref() == Some(&name) {
          return None;
        }
        return Some(s);
      })
      .collect();
  }

  // FIXME: Use hashed update to avoid races.
  state.validate_and_update_config(config, None).await?;

  return Ok(Json(serde_json::json!({})));
}
