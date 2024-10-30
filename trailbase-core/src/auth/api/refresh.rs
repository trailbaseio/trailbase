use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::tokens::reauth_with_refresh_token;
use crate::auth::AuthError;

#[derive(Debug, Deserialize, ToSchema, TS)]
#[ts(export)]
pub struct RefreshRequest {
  pub refresh_token: String,
}

#[derive(Debug, Serialize, ToSchema, TS)]
#[ts(export)]
pub struct RefreshResponse {
  pub auth_token: String,
  pub csrf_token: String,
}

/// Refreshes auth tokens given a refresh token.
///
/// NOTE: This is a json-only API, since cookies will be auto-refreshed.
#[utoipa::path(
  post,
  path = "/refresh",
  request_body = RefreshRequest,
  responses(
    (status = 200, description = "Refreshed auth tokens.", body = RefreshResponse)
  )
)]
pub(crate) async fn refresh_handler(
  State(state): State<AppState>,
  Json(request): Json<RefreshRequest>,
) -> Result<Json<RefreshResponse>, AuthError> {
  let (auth_token_ttl, refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());

  let claims = reauth_with_refresh_token(
    &state,
    request.refresh_token,
    refresh_token_ttl,
    auth_token_ttl,
  )
  .await?;

  let auth_token = state
    .jwt()
    .encode(&claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(Json(RefreshResponse {
    auth_token,
    csrf_token: claims.csrf_token,
  }));
}
