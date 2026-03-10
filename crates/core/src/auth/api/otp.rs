use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{Duration, Utc};
use const_format::formatcp;
use mini_moka::sync::Cache;
use serde::Deserialize;
use std::sync::LazyLock;
use tower_cookies::Cookies;
use trailbase_sqlite::params;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::api::login::{LoginResponse, build_auth_token_flow_response};
use crate::auth::util::{
  get_user_by_id, user_by_email, validate_and_normalize_email_address, validate_redirect,
};
use crate::constants::OTP_CODE_TABLE;
use crate::email::Email;
use crate::extract::Either;
use crate::rand::random_numeric_and_uppercase;
use crate::util::urlencode;

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct RequestOtpQuery {
  pub redirect_uri: Option<String>,
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct RequestOtpRequest {
  pub email: String,
  pub redirect_uri: Option<String>,
}

#[utoipa::path(
  post,
  path = "/otp/request",
  tag = "auth",
  params(RequestOtpQuery),
  request_body = RequestOtpRequest,
  responses(
    (status = 200, description = "OTP sent or user not found, when redirect_uri not present."),
    (status = 303, description = "OTP sent or user not found, when redirect_uri present."),
    (status = 400, description = "Bad request"),
    (status = 429, description = "Too many attempts"),
  )
)]

pub async fn request_otp_handler(
  State(state): State<AppState>,
  Query(query): Query<RequestOtpQuery>,
  either_request: Either<RequestOtpRequest>,
) -> Result<Response, AuthError> {
  if !state.access_config(|c| c.auth.enable_otp_signin()) {
    return Err(AuthError::MethodNotAllowed);
  }

  let (request, json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let redirect_uri = validate_redirect(&state, query.redirect_uri.or(request.redirect_uri))?;
  let normalized_email = validate_and_normalize_email_address(&request.email)?;

  {
    // Rate limit.
    if REQUEST_ATTEMPTS.get(&normalized_email).is_some() {
      return Err(AuthError::TooManyRequests);
    }
    REQUEST_ATTEMPTS.insert(normalized_email.clone(), ());
  }

  let success_response = || {
    if !json && let Some(ref redirect) = redirect_uri {
      return Redirect::to(&format!(
        "{redirect}?email={normalized_email}&alert={msg}",
        msg = urlencode("OTP sent")
      ))
      .into_response();
    }
    return (StatusCode::OK, "OTP sent").into_response();
  };

  let Ok(db_user) = user_by_email(&state, &normalized_email).await else {
    // In case we don't find a user we still reply with a success to avoid leaking
    // users' email addresses.
    return Ok(success_response());
  };

  if db_user.totp_secret.is_some() {
    // If the user has two/multi-factor-auth enabled, allowing OTP-only login would be a break of
    // contract. We may want to support OTP + TOTP going forward.
    return Ok(success_response());
  }

  let otp_code = random_numeric_and_uppercase(OTP_CODE_LENGTH);
  const UPDATE_OTP_QUERY: &str = formatcp!(
    "\
      INSERT OR REPLACE INTO '{OTP_CODE_TABLE}' (user, email, otp_code, expires) \
      VALUES ($1, $2, $3, $4) \
    "
  );

  let rows_affected = state
    .session_conn()
    .execute(
      UPDATE_OTP_QUERY,
      params!(
        db_user.id,
        normalized_email.clone(),
        otp_code.clone(),
        (Utc::now() + OTP_TTL).timestamp(),
      ),
    )
    .await?;

  if rows_affected != 1 {
    return Err(AuthError::Internal("Failed to insert OTP code".into()));
  }

  let email = Email::otp_email(&state, &db_user.email, &otp_code, redirect_uri.as_deref())
    .map_err(|err| AuthError::Internal(err.into()))?;
  email
    .send()
    .await
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(success_response());
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct LoginOtpQuery {
  pub email: Option<String>,
  pub code: Option<String>,
  pub redirect_uri: Option<String>,
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct LoginOtpRequest {
  pub email: Option<String>,
  pub code: Option<String>,
  pub redirect_uri: Option<String>,
}

#[utoipa::path(
  post,
  path = "/otp/login",
  tag = "auth",
  params(LoginOtpQuery),
  request_body = LoginOtpRequest,
  responses(
    (status = 200, description = "Auth tokens for JSONl logins.", body = LoginResponse),
    (status = 303, description = "For form logins."),
    (status = 400, description = "Bad request"),
  )
)]
pub async fn login_otp_handler(
  State(state): State<AppState>,
  cookies: Cookies,
  Query(query): Query<LoginOtpQuery>,
  either_request: Either<LoginOtpRequest>,
) -> Result<Response, AuthError> {
  if !state.access_config(|c| c.auth.enable_otp_signin()) {
    return Err(AuthError::MethodNotAllowed);
  }

  let (request, json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let redirect_uri = validate_redirect(&state, query.redirect_uri.or(request.redirect_uri))?;

  let Some(email) = query.email.as_deref().or(request.email.as_deref()) else {
    // TODO: Add redirect for non-json
    return Err(AuthError::BadRequest("missing email"));
  };
  let normalized_email = validate_and_normalize_email_address(email)?;

  let Some(otp_code) = query
    .code
    .as_deref()
    .or(request.code.as_deref())
    .map(|c| c.trim())
  else {
    // TODO: Add redirect for non-json
    return Err(AuthError::BadRequest("missing code"));
  };

  {
    // Rate limit.
    if let Some(attempts) = LOGIN_ATTEMPTS.get(&normalized_email) {
      if attempts >= 3 {
        return Err(AuthError::TooManyRequests);
      }
      LOGIN_ATTEMPTS.insert(normalized_email.clone(), attempts + 1);
    } else {
      LOGIN_ATTEMPTS.insert(normalized_email.clone(), 1);
    }
  }

  const LOOKUP_OTP_QUERY: &str = formatcp!(
    "\
      SELECT user FROM '{OTP_CODE_TABLE}' \
      WHERE  \
        email = $1 AND \
        otp_code = $2 AND \
        expires > UNIXEPOCH() \
    "
  );

  let Some(user_id) = state
    .session_conn()
    .query_row_f(
      LOOKUP_OTP_QUERY,
      params!(normalized_email, otp_code.to_string()),
      |row| row.get::<_, [u8; 16]>(0),
    )
    .await?
  else {
    return Err(AuthError::Unauthorized);
  };

  let db_user = get_user_by_id(state.user_conn(), &Uuid::from_bytes(user_id)).await?;

  return build_auth_token_flow_response(
    &state,
    &db_user,
    &cookies,
    redirect_uri.map(|uri| uri.to_string()),
    json,
  )
  .await;
}

const OTP_CODE_LENGTH: usize = 6;
const OTP_TTL: Duration = Duration::minutes(5);

// Track attempts to request OTP codes for abuse prevention.
static REQUEST_ATTEMPTS: LazyLock<Cache<String, ()>> = LazyLock::new(|| {
  Cache::builder()
    .time_to_live(std::time::Duration::from_secs(OTP_TTL.num_seconds() as u64))
    .max_capacity(2048)
    .build()
});

// Track login attempts for abuse prevention.
static LOGIN_ATTEMPTS: LazyLock<Cache<String, usize>> = LazyLock::new(|| {
  Cache::builder()
    .time_to_live(std::time::Duration::from_secs(60))
    .max_capacity(2048)
    .build()
});
