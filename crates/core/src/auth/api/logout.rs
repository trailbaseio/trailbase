use axum::extract::{Extension, Json, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::TypedHeader;
use serde::Deserialize;
use tower_cookies::Cookies;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::auth::user::User;
use crate::auth::util::{
  delete_all_sessions_for_user, delete_session, remove_all_cookies, validate_redirect,
};
use crate::auth::{AuthError, HasRoot};

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct LogoutParams {
  redirect_uri: Option<String>,
}

/// Logs out the current user and delete **all** pending sessions for that user.
///
/// Relies on the client to drop any auth tokens. We delete the session to avoid refresh tokens
/// bringing a logged out session back to live.
#[utoipa::path(
  get,
  path = "/logout",
  tag = "auth",
  params(LogoutParams),
  responses(
    (status = 200, description = "Sessions deleted. JWT auth token remains valid until expiry but refresh is no longer possible."),
    (status = 303, description = "Sessions deleted. JWT auth token remains valid until expiry but refresh is no longer possible. (?redirect_to= provided or non-json Content-Type with root path installed)"),
    (status = 401, description = "Failed to delete sessions (missing or invalid JWT)."),
  )
)]
pub async fn logout_handler(
  State(state): State<AppState>,
  Query(query): Query<LogoutParams>,
  Extension(HasRoot(has_root)): Extension<HasRoot>,
  content_type: Option<TypedHeader<headers::ContentType>>,
  user: User,
  cookies: Cookies,
) -> Result<Response, AuthError> {
  let redirect_uri = validate_redirect(&state, query.redirect_uri)?;

  delete_all_sessions_for_user(state.session_conn(), user.id).await?;

  remove_all_cookies(&cookies);

  if let Some(redirect) = redirect_uri {
    return Ok(Redirect::to(&redirect).into_response());
  }

  if has_root {
    let is_json =
      content_type.is_some_and(|TypedHeader(c)| mime::Mime::from(c) == mime::APPLICATION_JSON);
    if !is_json {
      return Ok(Redirect::to("/").into_response());
    }
  }

  return Ok((StatusCode::OK, "logged out").into_response());
}

#[derive(Clone, Debug, Deserialize, ToSchema, TS)]
#[ts(export)]
pub struct LogoutRequest {
  pub refresh_token: String,
}

/// Logs out the current user and deletes the specific session for the given refresh token.
///
/// Relies on the client to drop any auth tokens.
#[utoipa::path(
  post,
  path = "/logout",
  tag = "auth",
  request_body = LogoutRequest,
  responses(
    (status = 200, description = "Auth & refresh tokens.")
  )
)]
pub async fn post_logout_handler(
  State(state): State<AppState>,
  Json(request): Json<LogoutRequest>,
) -> Result<Response, AuthError> {
  delete_session(&state, request.refresh_token).await?;
  return Ok(StatusCode::OK.into_response());
}
