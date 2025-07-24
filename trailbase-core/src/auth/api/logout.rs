use axum::{
  extract::{Json, Query, State},
  http::StatusCode,
  response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use tower_cookies::Cookies;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::user::User;
use crate::auth::util::{
  delete_all_sessions_for_user, delete_session, remove_all_cookies, validate_redirects,
};

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct LogoutQuery {
  redirect_to: Option<String>,
}

/// Logs out the current user and delete **all** pending sessions for that user.
///
/// Relies on the client to drop any auth tokens. We delete the session to avoid refresh tokens
/// bringing a logged out session back to live.
#[utoipa::path(
  get,
  path = "/logout",
  tag = "auth",
  params(LogoutQuery),
  responses(
    (status = 200, description = "Auth & refresh tokens.")
  )
)]
pub async fn logout_handler(
  State(state): State<AppState>,
  Query(query): Query<LogoutQuery>,
  user: Option<User>,
  cookies: Cookies,
) -> Result<Redirect, AuthError> {
  let redirect = validate_redirects(&state, query.redirect_to.as_deref(), None)?;

  remove_all_cookies(&cookies);

  if let Some(user) = user {
    delete_all_sessions_for_user(state.user_conn(), user.uuid).await?;
  }

  return Ok(Redirect::to(redirect.as_deref().unwrap_or_else(|| {
    if state.public_dir().is_some() {
      "/"
    } else {
      "/_/auth/login"
    }
  })));
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
