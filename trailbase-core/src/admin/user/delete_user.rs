use axum::{
  extract::State,
  http::StatusCode,
  response::{IntoResponse, Response},
  Json,
};
use serde::Deserialize;
use ts_rs::TS;

use crate::admin::rows::delete_row;
use crate::admin::user::is_demo_admin;
use crate::admin::AdminError as Error;
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
    "_user",
    "id",
    serde_json::Value::String(uuid_to_b64(&request.id)),
  )
  .await?;

  return Ok((StatusCode::OK, "deleted").into_response());
}
