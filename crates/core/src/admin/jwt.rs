use axum::extract::State;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;

#[utoipa::path(
  get,
  path = "/public_key",
  tag = "admin",
  responses(
    (status = 200, description = "Success"),
  )
)]
pub async fn get_public_key(State(state): State<AppState>) -> Result<String, Error> {
  return Ok(state.jwt().public_key());
}
