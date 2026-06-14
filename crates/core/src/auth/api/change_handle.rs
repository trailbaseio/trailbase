use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use const_format::formatcp;
use serde::Deserialize;
use trailbase_sqlite::named_params;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::app_state::AppState;
use crate::auth::util::{validate_and_normalize_handle, validate_redirect};
use crate::auth::{AuthError, User};
use crate::constants::USER_TABLE;
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
  pub new_handle: Option<String>,

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
  Query(query): Query<ChangeHandleParams>,
  user: User,
  either_request: Either<ChangeHandleRequest>,
) -> Result<Response, AuthError> {
  if state.demo_mode() {
    return Err(AuthError::BadRequest("Disallowed in demo"));
  }

  let config_handles_allowed = false;
  if !config_handles_allowed {
    return Ok(StatusCode::METHOD_NOT_ALLOWED.into_response());
  }

  let (ChangeHandleRequest { new_handle, params }, json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let redirect_uri = validate_redirect(&state, query.redirect_uri.or(params.redirect_uri))?;
  let new_handle = if let Some(new_handle) = new_handle {
    validate_and_normalize_handle(&new_handle)?
  } else {
    return Err(AuthError::FailedDependency(
      "Un-setting not yet implemented".into(),
    ));
  };

  const UPDATE_HANDLE_QUERY: &str = formatcp!(
    "\
      UPDATE \"{USER_TABLE}\" \
        SET handle = :new_handle \
      WHERE \
        handle = :old_handle AND email = :email; \
    "
  );

  let rows_affected = state
    .user_conn()
    .execute(
      UPDATE_HANDLE_QUERY,
      named_params! {
        ":old_handle": user.handle,
        ":new_handle": new_handle,
        ":email": user.email,
      },
    )
    .await?;

  return match rows_affected {
    0 => Err(AuthError::Internal("update failed".into())),
    1 => {
      if !json && let Some(redirect_uri) = redirect_uri {
        Ok(Redirect::to(&redirect_uri).into_response())
      } else {
        Ok(StatusCode::OK.into_response())
      }
    }
    _ => {
      panic!("handle update affected multiple users: {rows_affected}");
    }
  };
}
