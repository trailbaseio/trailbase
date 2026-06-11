use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::app_state::AppState;
use crate::auth::{AuthError, User};
use crate::extract::Either;

#[derive(Debug, Default, Deserialize, IntoParams, ToSchema, TS)]
pub(crate) struct ChangeHandleParams {
  /// Success (and error if err_redirect_uri not present) redirect target for non-JSON requests.
  pub redirect_uri: Option<String>,
  /// Error redirect target for non-JSON requests.
  pub err_redirect_uri: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema, TS)]
#[ts(export)]
pub struct ChangeHandleRequest {
  pub new_handle: String,

  #[serde(flatten)]
  pub params: ChangeHandleParams,
}

/// Request a change of a user handle (e.g. username).
#[utoipa::path(
  post,
  path = "/change_handle",
  tag = "auth",
  params(ChangeHandleParams),
  request_body = ChangeHandleRequest,
  responses(
    (status = 200, description = "Success, when redirect_uri not present."),
    (status = 303, description = "Success, when redirect_uri present."),
  )
)]
pub async fn change_user_handle_handler(
  State(state): State<AppState>,
  Query(_query): Query<ChangeHandleParams>,
  _user: User,
  either_request: Either<ChangeHandleRequest>,
) -> Result<Response, AuthError> {
  if state.demo_mode() {
    return Err(AuthError::BadRequest("Disallowed in demo"));
  }

  let config_handles_allowed = false;
  if !config_handles_allowed {
    return Ok(StatusCode::METHOD_NOT_ALLOWED.into_response());
  }

  let (_request, _json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  return Err(AuthError::Internal("Not implemented".into()));
}
