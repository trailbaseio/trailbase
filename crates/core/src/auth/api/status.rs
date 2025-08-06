use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::tokens::{Tokens, reauth_with_refresh_token};

#[derive(Debug, Serialize, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct LoginStatusResponse {
  pub auth_token: Option<String>,
  pub refresh_token: Option<String>,
  pub csrf_token: Option<String>,
}

/// Check login status.
#[utoipa::path(
  get,
  path = "/status",
  tag = "auth",
  responses(
    (status = 200, description = "Auth & refresh tokens.", body = LoginStatusResponse)
  )
)]
pub(crate) async fn login_status_handler(
  State(state): State<AppState>,
  tokens: Option<Tokens>,
) -> Result<Json<LoginStatusResponse>, AuthError> {
  let Some(Tokens {
    auth_token_claims,
    refresh_token,
  }) = tokens
  else {
    // Return Ok but all Nones.
    return Ok(Json(LoginStatusResponse {
      auth_token: None,
      refresh_token: None,
      csrf_token: None,
    }));
  };

  // Decoding the auth token into its claims, already validated the therein contained expiration
  // time (exp). But rather than just re-encoding it, we refresh it. This ensures that the
  // session is still alive.
  if let Some(refresh_token) = refresh_token {
    let (auth_token_ttl, refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());
    let claims = reauth_with_refresh_token(
      &state,
      refresh_token.clone(),
      refresh_token_ttl,
      auth_token_ttl,
    )
    .await?;

    let auth_token = state
      .jwt()
      .encode(&claims)
      .map_err(|err| AuthError::Internal(err.into()))?;

    return Ok(Json(LoginStatusResponse {
      auth_token: Some(auth_token),
      refresh_token: Some(refresh_token),
      csrf_token: Some(claims.csrf_token),
    }));
  } else {
    // Fall back case: we don't have a refresh token so we cannot validate if a session is still
    // alive. We could look-up sessions by user id, however there can be more than one session
    // per user. Right now we return an OK with the original, re-encoded token. It may also make
    // sense to return an error here, i.e. consider this entire API more of a session status rather
    // than a token status.
    let auth_token = state
      .jwt()
      .encode(&auth_token_claims)
      .map_err(|err| AuthError::Internal(err.into()))?;

    return Ok(Json(LoginStatusResponse {
      auth_token: Some(auth_token),
      refresh_token: None,
      csrf_token: Some(auth_token_claims.csrf_token),
    }));
  }
}
