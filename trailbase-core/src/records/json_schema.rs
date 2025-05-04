use axum::extract::{Json, Path, Query, State};
use serde::Deserialize;
use trailbase_schema::json_schema::{
  Expand, JsonSchemaMode, build_json_schema, build_json_schema_expanded,
};

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::records::{Permission, RecordApi, RecordError};

#[derive(Debug, Clone, Deserialize)]
pub struct JsonSchemaQuery {
  pub mode: Option<JsonSchemaMode>,
}

/// Retrieve json schema associated with given record api.
#[utoipa::path(
  get,
  path = "/:name/schema",
  responses(
    (status = 200, description = "JSON schema.")
  )
)]
pub async fn json_schema_handler(
  State(state): State<AppState>,
  Path(api_name): Path<String>,
  Query(request): Query<JsonSchemaQuery>,
  user: Option<User>,
) -> Result<Json<serde_json::Value>, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  api
    .check_record_level_access(Permission::Schema, None, None, user.as_ref())
    .await?;

  return Ok(Json(build_api_json_schema(&state, &api, request.mode)?));
}

pub fn build_api_json_schema(
  state: &AppState,
  api: &RecordApi,
  mode: Option<JsonSchemaMode>,
) -> Result<serde_json::Value, RecordError> {
  let mode = mode.unwrap_or(JsonSchemaMode::Insert);

  if let (Some(config_expand), JsonSchemaMode::Select) = (api.expand(), mode) {
    let all_tables = state.schema_metadata().tables();
    let foreign_key_columns = config_expand.keys().map(|k| k.as_str()).collect::<Vec<_>>();
    let expand = Expand {
      tables: &all_tables,
      foreign_key_columns,
    };

    let (_schema, json) =
      build_json_schema_expanded(api.api_name(), api.columns(), mode, Some(expand))
        .map_err(|err| RecordError::Internal(err.into()))?;
    return Ok(json);
  }

  let (_schema, json) = build_json_schema(api.api_name(), api.columns(), mode)
    .map_err(|err| RecordError::Internal(err.into()))?;

  return Ok(json);
}
