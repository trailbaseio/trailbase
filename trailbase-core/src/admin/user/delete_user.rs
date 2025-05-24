use axum::{
  Json,
  extract::State,
  http::StatusCode,
  response::{IntoResponse, Response},
};
use serde::Deserialize;
use trailbase_schema::QualifiedName;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::admin::rows::delete_row;
use crate::admin::user::is_demo_admin;
use crate::app_state::AppState;
use crate::util::uuid_to_b64;

#[derive(Debug, Deserialize, Default, TS)]
#[ts(export)]
pub struct DeleteUserRequest {
  id: uuid::Uuid,
}

pub async fn delete_user_handler(
  State(state): State<AppState>,
  Json(request): Json<DeleteUserRequest>,
) -> Result<Response, Error> {
  if state.demo_mode() && is_demo_admin(&state, &request.id).await {
    return Err(Error::Precondition("Deleting demo admin forbidden".into()));
  }

  delete_row(
    &state,
    &QualifiedName {
      name: "_user".to_string(),
      database_schema: None,
    },
    "id",
    serde_json::Value::String(uuid_to_b64(&request.id)),
  )
  .await?;

  return Ok((StatusCode::OK, "deleted").into_response());
}
