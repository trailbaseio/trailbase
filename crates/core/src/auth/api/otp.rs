use axum::{
  extract::{Json, State},
  http::StatusCode,
  response::{IntoResponse, Response},
};
use chrono::Duration;
use const_format::formatcp;
use serde::Deserialize;
use trailbase_sqlite::params;
use ts_rs::TS;
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::api::login::LoginResponse;
use crate::auth::tokens::mint_new_tokens;
use crate::auth::user::DbUser;
use crate::auth::util::{user_by_email, validate_and_normalize_email_address};
use crate::constants::{OTP_LENGTH, USER_TABLE};
use crate::email::Email;
use crate::rand::generate_random_string;

const OTP_TTL_SEC: i64 = 300; // 5 minutes
const OTP_RATE_LIMIT_SEC: i64 = 60; // 1 minute

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct RequestOTPRequest {
  pub email: String,
}

#[utoipa::path(
  post,
  path = "/otp/request",
  tag = "auth",
  request_body = RequestOTPRequest,
  responses(
    (status = 200, description = "OTP sent.")
  )
)]

pub async fn request_otp_handler(
  State(state): State<AppState>,
  Json(params): Json<RequestOTPRequest>,
) -> Result<Response, AuthError> {
  let email = validate_and_normalize_email_address(&params.email)?;
  let db_user = user_by_email(&state, &email).await?;

  if let Some(last_sent) = db_user.otp_sent_at {
    let Some(timestamp) = chrono::DateTime::from_timestamp(last_sent, 0) else {
      return Err(AuthError::Internal("Invalid timestamp".into()));
    };
    let age: Duration = chrono::Utc::now() - timestamp;
    if age < Duration::seconds(OTP_RATE_LIMIT_SEC) {
      return Err(AuthError::TooManyRequests);
    }
  }

  let otp_code = generate_random_string(OTP_LENGTH);
  const UPDATE_OTP_QUERY: &str = formatcp!(
    "\
      UPDATE '{USER_TABLE}' \
      SET \
        otp_code = $1, \
        otp_sent_at = UNIXEPOCH() \
      WHERE \
        id = $2 \
    "
  );

  let rows_affected = state
    .user_conn()
    .execute(UPDATE_OTP_QUERY, params!(otp_code.clone(), db_user.id))
    .await?;

  if rows_affected != 1 {
    return Err(AuthError::Internal("Failed to update user OTP".into()));
  }

  let email = Email::otp_email(&state, &db_user.email, &otp_code)
    .map_err(|err| AuthError::Internal(err.into()))?;
  email
    .send()
    .await
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok((StatusCode::OK, "OTP sent").into_response());
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct VerifyOTPRequest {
  pub email: String,
  pub code: String,
}

#[utoipa::path(
  post,
  path = "/otp/verify",
  tag = "auth",
  request_body = VerifyOTPRequest,
  responses(
    (status = 200, description = "Auth tokens.", body = LoginResponse)
  )
)]
pub async fn verify_otp_handler(
  State(state): State<AppState>,
  Json(params): Json<VerifyOTPRequest>,
) -> Result<Response, AuthError> {
  let email = validate_and_normalize_email_address(&params.email)?;
  let db_user = user_by_email(&state, &email).await?;

  verify_otp_code(&db_user, &params.code)?;

  if db_user.totp_secret.is_some() {
    return Err(AuthError::BadRequest("TOTP_REQUIRED"));
  }

  let mut updated_user = db_user.clone();
  updated_user.verified = true;

  let (auth_token_ttl, _refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());

  let tokens = mint_new_tokens(state.user_conn(), &updated_user, auth_token_ttl).await?;

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

pub fn verify_otp_code(user: &DbUser, code: &str) -> Result<(), AuthError> {
  if let (Some(expected_code), Some(sent_at)) = (&user.otp_code, user.otp_sent_at) {
    if expected_code != code {
      return Err(AuthError::BadRequest("invalid user"));
    }

    let Some(timestamp) = chrono::DateTime::from_timestamp(sent_at, 0) else {
      return Err(AuthError::Internal("Invalid timestamp".into()));
    };
    let age: Duration = chrono::Utc::now() - timestamp;
    if age > Duration::seconds(OTP_TTL_SEC) {
      return Err(AuthError::BadRequest("OTP expired"));
    }

    Ok(())
  } else {
    return Err(AuthError::BadRequest("invalid user"));
  }
}
