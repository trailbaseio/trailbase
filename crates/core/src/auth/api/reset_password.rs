use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use const_format::formatcp;
use mini_moka::sync::Cache;
use serde::Deserialize;
use std::sync::LazyLock;
use trailbase_sqlite::params;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::jwt::PasswordResetTokenClaims;
use crate::auth::password::{hash_password, validate_password_policy};
use crate::auth::util::{user_by_email, validate_and_normalize_email_address, validate_redirect};
use crate::constants::USER_TABLE;
use crate::email::Email;
use crate::extract::Either;
use crate::util::urlencode;

const TTL_SEC: i64 = 3600;
const RATE_LIMIT_SEC: i64 = 3600;

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ResetPasswordQuery {
  redirect_uri: Option<String>,
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
pub struct ResetPasswordRequest {
  pub email: String,
  pub redirect_uri: Option<String>,
}

/// Request a password reset.
#[utoipa::path(
  post,
  path = "/reset_password/request",
  tag = "auth",
  params(ResetPasswordQuery),
  request_body = ResetPasswordRequest,
  responses(
    (status = 200, description = "Success or user not found, when redirect_uri not present."),
    (status = 303, description = "Success or user not found, when redirect_uri present"),
    (status = 400, description = "Malformed email address."),
  )
)]
pub async fn reset_password_request_handler(
  State(state): State<AppState>,
  query: Query<ResetPasswordQuery>,
  either_request: Either<ResetPasswordRequest>,
) -> Result<Response, AuthError> {
  let request = match either_request {
    Either::Json(req) => req,
    Either::Multipart(req, _) => req,
    Either::Form(req) => req,
  };

  let redirect_uri = validate_redirect(
    &state,
    query
      .redirect_uri
      .as_deref()
      .or(request.redirect_uri.as_deref()),
  )?;
  let normalized_email = validate_and_normalize_email_address(&request.email)?;

  {
    // Rate limit.
    if ATTEMPTS.get(&normalized_email).is_some() {
      return Err(AuthError::TooManyRequests);
    }
    ATTEMPTS.insert(normalized_email.clone(), ());
  }

  let success_response = || {
    if let Some(redirect) = redirect_uri {
      return Redirect::to(&format!(
        "{redirect}?alert={msg}",
        msg = urlencode("Password reset email sent")
      ))
      .into_response();
    }
    return (StatusCode::OK, "Password reset email sent").into_response();
  };
  let Ok(user) = user_by_email(&state, &normalized_email).await else {
    // In case we don't find a user we still reply with a success to avoid leaking
    // users' email addresses.
    return Ok(success_response());
  };

  let password_reset_claims =
    PasswordResetTokenClaims::new(&normalized_email, chrono::Duration::seconds(TTL_SEC));
  let token = state
    .jwt()
    .encode(&password_reset_claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  let email = Email::password_reset_email(&state, &user.email, &token)
    .map_err(|err| AuthError::Internal(err.into()))?;
  email
    .send()
    .await
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(success_response());
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ResetPasswordUpdateQuery {
  redirect_uri: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ResetPasswordUpdateRequest {
  pub password: String,
  pub password_repeat: String,

  // NOTE: This is a token. It used to be a code.
  #[serde(alias = "password_reset_code")]
  pub password_reset_token: String,

  pub redirect_uri: Option<String>,
}

/// Endpoint for setting a new password after the user has requested a reset and provided a
/// replacement password.
#[utoipa::path(
  post,
  path = "/reset_password/update",
  tag = "auth",
  params(ResetPasswordUpdateQuery),
  request_body = ResetPasswordUpdateRequest,
  responses(
    (status = 303, description = "Success. Redirect to provided `redirect_uri` or login page."),
    (status = 400, description = "Bad request: invalid redirect_uri."),
    (status = 401, description = "Unauthorized: invalid reset code."),
  )
)]
pub async fn reset_password_update_handler(
  State(state): State<AppState>,
  query: Query<ResetPasswordUpdateQuery>,
  either_request: Either<ResetPasswordUpdateRequest>,
) -> Result<Response, AuthError> {
  let request = match either_request {
    Either::Json(req) => req,
    Either::Multipart(req, _) => req,
    Either::Form(req) => req,
  };

  let redirect_uri = validate_redirect(
    &state,
    query
      .redirect_uri
      .as_deref()
      .or(request.redirect_uri.as_deref()),
  )?;

  let auth_options = state.auth_options();
  validate_password_policy(
    &request.password,
    &request.password_repeat,
    auth_options.password_options(),
  )?;

  let password_reset_claims =
    PasswordResetTokenClaims::from_password_reset_token(state.jwt(), &request.password_reset_token)
      .map_err(|_err| AuthError::BadRequest("Invalid token"))?;

  let hashed_password = hash_password(&request.password)?;
  const UPDATE_PASSWORD_QUERY: &str = formatcp!(
    "\
      UPDATE '{USER_TABLE}' \
      SET password_hash = $1 \
      WHERE email = $2 \
    "
  );

  let rows_affected = state
    .user_conn()
    .execute(
      UPDATE_PASSWORD_QUERY,
      params!(hashed_password, password_reset_claims.sub),
    )
    .await?;

  return match rows_affected {
    0 => Err(AuthError::Unauthorized),
    1 => {
      if let Some(redirect) = redirect_uri {
        Ok(
          Redirect::to(&format!(
            "{redirect}?alert={msg}",
            msg = urlencode("Password reset")
          ))
          .into_response(),
        )
      } else {
        Ok((StatusCode::OK, "Password reset").into_response())
      }
    }
    _ => {
      panic!("multiple users with same verification code.");
    }
  };
}

// Track login attempts for abuse prevention.
static ATTEMPTS: LazyLock<Cache<String, ()>> = LazyLock::new(|| {
  Cache::builder()
    .time_to_live(std::time::Duration::from_secs(RATE_LIMIT_SEC as u64))
    .max_capacity(2048)
    .build()
});
