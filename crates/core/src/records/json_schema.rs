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
  path = "/{name}/schema",
  tag = "records",
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

  api.check_table_level_access(Permission::Schema, user.as_ref())?;

  // NOTE:  Micro optimization: since there's no subsequent DB interaction, we could first check if
  // there even is a schema access rule and only then invoke `call_reader`.
  api
    .conn()
    .call_reader({
      let api = api.clone();

      move |mut conn| {
        return api.check_record_level_access_only(
          &mut conn,
          Permission::Schema,
          None,
          None,
          user.as_ref(),
        );
      }
    })
    .await?;

  return Ok(Json(build_api_json_schema(&state, &api, request.mode)?));
}

pub(super) fn build_api_json_schema_internal(
  state: &AppState,
  api: &RecordApi,
  mode: JsonSchemaMode,
) -> Result<(jsonschema::Validator, serde_json::Value), RecordError> {
  let config_expand = api.expand();
  if mode == JsonSchemaMode::Select && !config_expand.is_empty() {
    let metadata = api.connection_metadata();
    let all_tables: Vec<_> = metadata.tables.values().collect();

    let expand = Expand {
      tables: &all_tables,
      foreign_key_columns: config_expand.iter().map(|c| c.as_str()).collect(),
    };

    return build_json_schema_expanded(
      &state.json_schema_registry().load(),
      api.api_name(),
      api.columns(),
      mode,
      Some(expand),
    )
    .map_err(|err| RecordError::Internal(err.into()));
  }

  return build_json_schema(
    &state.json_schema_registry().load(),
    api.api_name(),
    api.columns(),
    mode,
  )
  .map_err(|err| RecordError::Internal(err.into()));
}

pub fn build_api_json_schema(
  state: &AppState,
  api: &RecordApi,
  mode: Option<JsonSchemaMode>,
) -> Result<serde_json::Value, RecordError> {
  let (_validator, json) =
    build_api_json_schema_internal(state, api, mode.unwrap_or(JsonSchemaMode::Insert))?;
  return Ok(json);
}
