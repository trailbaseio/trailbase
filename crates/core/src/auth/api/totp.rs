use axum::{
  extract::{Json, State},
  http::StatusCode,
  response::{IntoResponse, Response},
};
use const_format::formatcp;
use serde::{Deserialize, Serialize};
use totp_rs::{Algorithm, Secret, TOTP};
use trailbase_sqlite::params;
use ts_rs::TS;
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::api::login::LoginResponse;
use crate::auth::password::check_user_password;
use crate::auth::tokens::mint_new_tokens;
use crate::auth::util::{user_by_email, validate_and_normalize_email_address};
use crate::auth::{AuthError, User};
use crate::constants::USER_TABLE;
use crate::extract::Either;

#[derive(Debug, Deserialize, Serialize, ToSchema, TS)]
#[ts(export)]
pub struct RegisterTotpResponse {
  // FIXME: This won't work for auth-ui. We'll need a redirect and SSG QR.
  pub totp_url: String,
}

/// Sign-up user for TOTP second factor.
#[utoipa::path(
  get,
  path = "/totp/register",
  tag = "auth",
  responses(
    (status = 200, description = "TOTP secret and QR code URI.", body = RegisterTotpResponse)
  )
)]
pub async fn register_totp_request_handler(
  State(state): State<AppState>,
  user: User,
) -> Result<Response, AuthError> {
  let secret = Secret::generate_secret();

  // Generate QR code URI for authenticator apps
  let app_name = state
    .access_config(|c| c.server.application_name.clone())
    .unwrap_or_else(|| "TrailBase".to_string());

  let totp = new_totp(&secret, Some(&app_name), Some(&user.email))?;

  let json = true;
  if !json {
    // let qr_code = totp.get_qr_png().unwrap();
  }

  return Ok(
    Json(RegisterTotpResponse {
      totp_url: totp.get_url(),
    })
    .into_response(),
  );
}

#[derive(Debug, Deserialize, Serialize, ToSchema, TS)]
#[ts(export)]
pub struct VerifyRegisterTotpRequest {
  pub totp_url: String,
  pub totp: String,
}

/// Verify the current user's TOTP
#[utoipa::path(
  post,
  path = "/totp/confirm",
  tag = "auth",
  request_body = VerifyRegisterTotpRequest,
  responses(
    (status = 200, description = "TOTP verified")
  )
)]
pub async fn register_totp_confirm_handler(
  State(state): State<AppState>,
  user: User,
  either_request: Either<VerifyRegisterTotpRequest>,
) -> Result<Response, AuthError> {
  let (request, _json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let totp =
    TOTP::from_url(&request.totp_url).map_err(|_err| AuthError::BadRequest("invalid totp url"))?;
  if !totp.check_current(&request.totp).unwrap_or(false) {
    return Err(AuthError::BadRequest("invalid totp code"));
  }

  const UPDATE_QUERY: &str = formatcp!("UPDATE '{USER_TABLE}' SET totp_secret = $1 WHERE id = $2");

  let user_id_bytes = user.uuid.into_bytes().to_vec();
  let secret = totp.get_secret_base32();

  state
    .user_conn()
    .execute(UPDATE_QUERY, params!(secret, user_id_bytes))
    .await?;

  return Ok((StatusCode::OK, "TOTP enabled").into_response());
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct DisableTotpRequest {
  pub totp: String,
}

#[utoipa::path(
  post,
  path = "/totp/disable",
  tag = "auth",
  request_body = DisableTotpRequest,
  responses(
    (status = 200, description = "TOTP disabled successfully.")
  )
)]
pub async fn disable_totp_handler(
  State(state): State<AppState>,
  user: User,
  either_request: Either<DisableTotpRequest>,
) -> Result<Response, AuthError> {
  let (request, _json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let db_user = user_by_email(&state, &user.email).await?;
  let Some(secret) = db_user.totp_secret.map(Secret::Encoded) else {
    return Err(AuthError::BadRequest("TOTP not enabled for this user"));
  };

  let totp = new_totp(&secret, None, None)?;

  if totp.check_current(&request.totp).unwrap_or(false) {
    const UPDATE_QUERY: &str =
      formatcp!("UPDATE '{USER_TABLE}' SET totp_secret = $1 WHERE id = $2");

    let user_id_bytes = user.uuid.into_bytes().to_vec();
    state
      .user_conn()
      .execute(UPDATE_QUERY, params!(Option::<String>::None, user_id_bytes))
      .await?;

    return Ok((StatusCode::OK, "TOTP disabled").into_response());
  }

  return Err(AuthError::BadRequest("Invalid TOTP code"));
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct LoginTotpRequest {
  pub email: String,
  pub totp: String,
  pub password: Option<String>,
  pub otp: Option<String>,
}

#[utoipa::path(
  post,
  path = "/totp/login",
  tag = "auth",
  request_body = LoginTotpRequest,
  responses(
    (status = 200, description = "Auth tokens.", body = LoginResponse)
  )
)]
pub async fn login_totp_handler(
  State(state): State<AppState>,
  either_request: Either<LoginTotpRequest>,
) -> Result<Response, AuthError> {
  let (request, _json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let email = validate_and_normalize_email_address(&request.email)?;
  let db_user = user_by_email(&state, &email).await?;
  let Some(secret) = db_user.totp_secret.to_owned().map(Secret::Encoded) else {
    return Err(AuthError::BadRequest("TOTP not enabled for this user"));
  };

  if let Some(password) = request.password {
    check_user_password(&db_user, &password, state.demo_mode())?;
  } else if let Some(otp) = request.otp {
    super::otp::verify_otp_code(&db_user, &otp)?;
  } else {
    return Err(AuthError::BadRequest("missing params"));
  }

  let totp = new_totp(&secret, None, None)?;

  if totp.check_current(&request.totp).unwrap_or(false) {
    let (auth_token_ttl, _refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());
    let tokens = mint_new_tokens(state.user_conn(), &db_user, auth_token_ttl).await?;

    let response = LoginResponse {
      auth_token: state
        .jwt()
        .encode(&tokens.auth_token_claims)
        .map_err(|err| AuthError::Internal(err.into()))?,
      refresh_token: tokens.refresh_token,
      csrf_token: tokens.auth_token_claims.csrf_token,
    };

    return Ok(Json(response).into_response());
  }
  return Err(AuthError::BadRequest("Invalid TOTP code"));
}

fn new_totp(
  secret: &Secret,
  app_name: Option<&str>,
  account: Option<&str>,
) -> Result<TOTP, AuthError> {
  return TOTP::new(
    Algorithm::SHA1,
    6,
    1,
    30,
    secret
      .to_bytes()
      .map_err(|err| AuthError::Internal(err.into()))?,
    app_name.map(|name| name.to_string()),
    account.unwrap_or_default().to_string(),
  )
  .map_err(|err| AuthError::Internal(err.into()));
}
