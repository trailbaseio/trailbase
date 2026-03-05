use axum::extract::{Json, State};
use const_format::formatcp;
use serde::{Deserialize, Serialize};
use trailbase_sqlite::params;
use ts_rs::TS;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::AuthError;
use crate::auth::tokens::mint_new_tokens;
use crate::auth::util::{derive_pkce_code_challenge, get_user_by_id};
use crate::constants::{AUTHORIZATION_CODE_TABLE, USER_TABLE, VERIFICATION_CODE_LENGTH};
use crate::{app_state::AppState, auth::user::DbUser};

#[derive(Clone, Debug, Deserialize, ToSchema, TS)]
#[ts(export)]
pub struct AuthCodeToTokenRequest {
  pub authorization_code: Option<String>,
  pub pkce_code_verifier: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TokenResponse {
  pub auth_token: String,
  pub refresh_token: String,
  pub csrf_token: String,
}

/// Exchange authorization code for auth tokens.
///
/// This API endpoint is meant for client-side applications (SPA, mobile, desktop, ...) using the
/// web-auth flow.
#[utoipa::path(
  post,
  path = "/token",
  tag = "auth",
  request_body = AuthCodeToTokenRequest,
  responses(
    (status = 200, description = "Converts auth & pkce codes to tokens.", body = TokenResponse)
  )
)]
pub(crate) async fn auth_code_to_token_handler(
  State(state): State<AppState>,
  Json(request): Json<AuthCodeToTokenRequest>,
) -> Result<Json<TokenResponse>, AuthError> {
  let authorization_code = match request.authorization_code {
    Some(code) if code.len() == VERIFICATION_CODE_LENGTH => code,
    _ => {
      return Err(AuthError::BadRequest("invalid auth code"));
    }
  };

  let pkce_code_challenge = request
    .pkce_code_verifier
    .as_ref()
    .map(|verifier| derive_pkce_code_challenge(verifier));

  const AUTH_CODE_QUERY: &str = formatcp!(
    "\
      SELECT user FROM '{AUTHORIZATION_CODE_TABLE}' \
      WHERE \
        authorization_code = $1 \
          AND pkce_code_challenge = $2 \
          AND expires > UNIXEPOCH() \
    "
  );

  let Some(user_id) = state
    .session_conn()
    .query_row_f(
      AUTH_CODE_QUERY,
      params!(authorization_code, pkce_code_challenge),
      |row| row.get::<_, [u8; 16]>(0),
    )
    .await?
  else {
    return Err(AuthError::NotFound);
  };

  let db_user = get_user_by_id(state.user_conn(), &Uuid::from_bytes(user_id)).await?;

  let (auth_token_ttl, _refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());

  let tokens = mint_new_tokens(state.session_conn(), &db_user, auth_token_ttl).await?;
  let auth_token = state
    .jwt()
    .encode(&tokens.auth_token_claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(Json(TokenResponse {
    auth_token,
    refresh_token: tokens.refresh_token,
    csrf_token: tokens.auth_token_claims.csrf_token,
  }));
}
