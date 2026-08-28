use axum::extract::{Json, State};
use serde::Deserialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::auth::api::login::LoginResponse;
use crate::auth::tokens::{FreshTokens, mint_new_tokens};

#[derive(Debug, Deserialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct MintRequest {
  /// User to mint tokens for.
  user: Uuid,
}

#[utoipa::path(
  post,
  path = "/mint",
  tag = "admin",
  request_body = MintRequest,
  responses(
    (status = 200, description = "Success", body = LoginResponse),
  )
)]
pub async fn mint_auth_tokens(
  State(state): State<AppState>,
  Json(request): Json<MintRequest>,
) -> Result<Json<LoginResponse>, Error> {
  let jwt = state.jwt();
  let db_user = crate::auth::util::user_by_id(&state, &request.user).await?;

  if db_user.admin {
    return Err(Error::Precondition("Not allowed for admins".into()));
  }

  let auth_token_ttl = chrono::Duration::hours(12);
  let refresh_token_ttl = chrono::Duration::hours(12);
  let FreshTokens {
    auth_token_claims,
    refresh_token,
  } = mint_new_tokens(
    state.session_conn(),
    &db_user,
    &auth_token_ttl,
    &refresh_token_ttl,
  )
  .await?;

  let auth_token = jwt
    .encode(&auth_token_claims)
    .map_err(|err| Error::Internal(err.into()))?;

  return Ok(Json(LoginResponse {
    auth_token,
    refresh_token,
    csrf_token: auth_token_claims.csrf_token,
  }));
}
