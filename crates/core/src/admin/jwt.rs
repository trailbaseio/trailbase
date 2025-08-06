use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::admin::AdminError as Error;
use crate::app_state::AppState;

pub async fn get_public_key(State(state): State<AppState>) -> Result<Response, Error> {
  return Ok((StatusCode::OK, state.jwt().public_key()).into_response());
}
