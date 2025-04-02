use axum::extract::{Json, Path, Query, State};
use serde::Deserialize;

use crate::auth::user::User;
use crate::records::{Permission, RecordError};
use crate::table_metadata::{build_json_schema, build_json_schema_recursive, Expand};
use crate::{api::JsonSchemaMode, app_state::AppState};

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
  let Some(columns) = api.metadata().columns() else {
    return Err(RecordError::Internal("Missing schema".into()));
  };

  api
    .check_record_level_access(Permission::Schema, None, None, user.as_ref())
    .await?;

  let mode = request.mode.unwrap_or(JsonSchemaMode::Insert);

  match (api.expand(), mode) {
    (Some(config_expand), JsonSchemaMode::Select) => {
      let foreign_key_columns = config_expand.keys().map(|k| k.as_str()).collect::<Vec<_>>();
      let expand = Expand {
        table_metadata: state.table_metadata(),
        foreign_key_columns,
      };

      let (_schema, json) =
        build_json_schema_recursive(api.table_name(), columns, mode, Some(expand))
          .map_err(|err| RecordError::Internal(err.into()))?;
      return Ok(Json(json));
    }
    _ => {
      let (_schema, json) = build_json_schema(api.table_name(), columns, mode)
        .map_err(|err| RecordError::Internal(err.into()))?;
      return Ok(Json(json));
    }
  }
}
