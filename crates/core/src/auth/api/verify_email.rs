use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use const_format::formatcp;
use mini_moka::sync::Cache;
use serde::Deserialize;
use std::sync::LazyLock;
use trailbase_sqlite::params;
use utoipa::IntoParams;

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::jwt::EmailVerificationTokenClaims;
use crate::auth::util::{user_by_email, validate_and_normalize_email_address, validate_redirect};
use crate::constants::USER_TABLE;
use crate::email::Email;
use crate::util::urlencode;

const TTL_SEC: i64 = 3600;
const RATE_LIMIT_SEC: i64 = 4 * 3600;

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct EmailVerificationQuery {
  pub email: String,
  pub redirect_uri: Option<String>,
}

/// Request a new email to verify email address.
#[utoipa::path(
  get,
  path = "/verify_email/trigger",
  tag = "auth",
  params(EmailVerificationQuery),
  responses(
    (status = 200, description = "Email verification sent or user not found, when redirect_uri not present."),
    (status = 303, description = "Email verification sent or user not found, when redirect_uri present."),
    (status = 400, description = "Malformed email address."),
  )
)]
pub async fn request_email_verification_handler(
  State(state): State<AppState>,
  Query(query): Query<EmailVerificationQuery>,
) -> Result<Response, AuthError> {
  let normalized_email = validate_and_normalize_email_address(&query.email)?;
  let redirect_uri = validate_redirect(&state, query.redirect_uri.as_deref())?;

  {
    // Rate limit.
    if ATTEMPTS.get(&normalized_email).is_some() {
      return Err(AuthError::TooManyRequests);
    }
    ATTEMPTS.insert(normalized_email.clone(), ());
  }

  let success_response = || {
    if let Some(redirect) = redirect_uri {
      Redirect::to(&format!(
        "{redirect}?alert={msg}",
        msg = urlencode("Verification email sent")
      ))
      .into_response()
    } else {
      (StatusCode::OK, "Verification email sent").into_response()
    }
  };
  let Ok(user) = user_by_email(&state, &normalized_email).await else {
    // In case we don't find a user we still reply with a success to avoid leaking
    // users' email addresses.
    return Ok(success_response());
  };

  let claims = EmailVerificationTokenClaims::new(
    &user.uuid(),
    user.email.clone(),
    chrono::Duration::seconds(TTL_SEC),
  );
  let token = state
    .jwt()
    .encode(&claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  let email = Email::verification_email(&state, &user.email, &token)
    .map_err(|err| AuthError::Internal(err.into()))?;
  email
    .send()
    .await
    .map_err(|err| AuthError::FailedDependency(format!("Failed to send Email {err}.").into()))?;

  return Ok(success_response());
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub(crate) struct VerifyEmailQuery {
  redirect_uri: Option<String>,
}

/// Request a new email to verify email address.
#[utoipa::path(
  get,
  path = "/verify_email/confirm/:email_verification_code",
  tag = "auth",
  responses(
    (status = 200, description = "Email verified, when redirect_uri not present"),
    (status = 303, description = "Email verified, when redirect_uri present"),
    (status = 400, description = "Bad request: invalid redirect_uri."),
    (status = 401, description = "Unauthorized: invalid reset code."),
  )
)]
pub(crate) async fn verify_email_handler(
  State(state): State<AppState>,
  Path(email_verification_token): Path<String>,
  query: Query<VerifyEmailQuery>,
) -> Result<Response, AuthError> {
  let redirect_uri = validate_redirect(&state, query.redirect_uri.as_deref())?;

  let claims = EmailVerificationTokenClaims::decode(state.jwt(), &email_verification_token)
    .map_err(|_err| AuthError::BadRequest("invalid token"))?;

  const UPDATE_CODE_QUERY: &str = formatcp!(
    "\
      UPDATE '{USER_TABLE}' \
      SET verified = TRUE \
      WHERE email = $2 \
    "
  );

  let rows_affected = state
    .user_conn()
    .execute(UPDATE_CODE_QUERY, params!(claims.email))
    .await?;

  return match rows_affected {
    0 => Err(AuthError::Unauthorized),
    1 => {
      if let Some(redirect) = redirect_uri {
        Ok(
          Redirect::to(&format!(
            "{redirect}?alert={msg}",
            msg = urlencode("email verified")
          ))
          .into_response(),
        )
      } else {
        Ok((StatusCode::OK, "email verified").into_response())
      }
    }
    _ => panic!("email verification affected multiple users: {rows_affected}"),
  };
}

// Track login attempts for abuse prevention.
static ATTEMPTS: LazyLock<Cache<String, ()>> = LazyLock::new(|| {
  Cache::builder()
    .time_to_live(std::time::Duration::from_secs(RATE_LIMIT_SEC as u64))
    .max_capacity(2048)
    .build()
});
