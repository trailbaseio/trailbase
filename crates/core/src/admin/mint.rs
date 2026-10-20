use axum::extract::{Json, State};
use serde::Deserialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::auth::api::login::LoginResponse;
use crate::auth::user::DbUserAndTenant;

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
  let db_user = crate::auth::util::user_by_id(&state, &request.user).await?;
  if db_user.admin {
    // Admins cannot impersonate other admins to prevent abuse. We do allow minting tokens for
    // admins from the command line, since system admins are considered higher privilege than
    // admins.
    return Err(Error::Precondition("Not allowed for admins".into()));
  }

  let crate::auth::cli::AuthTokens {
    auth_token,
    refresh_token,
    csrf_token,
  } = crate::auth::cli::mint_auth_tokens_impl(
    &state,
    &DbUserAndTenant {
      user: db_user,
      tenant: None,
    },
  )
  .await?;

  return Ok(Json(LoginResponse {
    auth_token,
    refresh_token,
    csrf_token,
  }));
}
