use axum::{
  extract::State,
  http::StatusCode,
  response::{IntoResponse, Response},
  Json,
};
use serde::Deserialize;
use ts_rs::TS;

use crate::admin::rows::delete_row;
use crate::admin::AdminError as Error;
use crate::app_state::AppState;

#[derive(Debug, Deserialize, Default, TS)]
#[ts(export)]
pub struct DeleteUserRequest {
  #[ts(type = "string")]
  id: serde_json::Value,
}

pub async fn delete_user_handler(
  State(state): State<AppState>,
  Json(request): Json<DeleteUserRequest>,
) -> Result<Response, Error> {
  delete_row(&state, "_user", "id", request.id).await?;

  return Ok((StatusCode::OK, "deleted").into_response());
}
