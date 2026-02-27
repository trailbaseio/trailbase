use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use const_format::formatcp;
use serde::Deserialize;
use trailbase_sqlite::params;
use utoipa::IntoParams;

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::util::{user_by_email, validate_and_normalize_email_address, validate_redirect};
use crate::constants::{USER_TABLE, VERIFICATION_CODE_LENGTH};
use crate::email::Email;
use crate::rand::generate_random_string;
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

  if let Some(last_verification) = user.email_verification_code_sent_at {
    let Some(timestamp) = chrono::DateTime::from_timestamp(last_verification, 0) else {
      return Err(AuthError::Internal("Invalid timestamp".into()));
    };

    let age = chrono::Utc::now() - timestamp;
    if age < chrono::Duration::seconds(RATE_LIMIT_SEC) {
      return Err(AuthError::TooManyRequests);
    }
  }

  let email_verification_code = generate_random_string(VERIFICATION_CODE_LENGTH);
  const UPDATE_VERIFICATION_CODE_QUERY: &str = formatcp!(
    "\
      UPDATE '{USER_TABLE}' \
      SET \
        email_verification_code = $1, \
        email_verification_code_sent_at = UNIXEPOCH() \
      WHERE \
        id = $2 \
    "
  );

  let rows_affected = state
    .user_conn()
    .execute(
      UPDATE_VERIFICATION_CODE_QUERY,
      params!(email_verification_code.clone(), user.id),
    )
    .await?;

  return match rows_affected {
    // Race: can only happen if user is removed while email is verified.
    0 => Err(AuthError::Conflict),
    1 => {
      let email = Email::verification_email(&state, &user.email, &email_verification_code)
        .map_err(|err| AuthError::Internal(err.into()))?;
      email
        .send()
        .await
        .map_err(|err| AuthError::Internal(err.into()))?;

      Ok(success_response())
    }
    _ => {
      panic!("Password reset affected multiple users: {rows_affected}");
    }
  };
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
  Path(email_verification_code): Path<String>,
  query: Query<VerifyEmailQuery>,
) -> Result<Response, AuthError> {
  let redirect_uri = validate_redirect(&state, query.redirect_uri.as_deref())?;

  const UPDATE_CODE_QUERY: &str = formatcp!(
    "\
      UPDATE '{USER_TABLE}' \
      SET \
        verified = TRUE, \
        email_verification_code = NULL, \
        email_verification_code_sent_at = NULL \
      WHERE \
        email_verification_code = $1 AND email_verification_code_sent_at > (UNIXEPOCH() - {TTL_SEC}) \
    "
  );

  let rows_affected = state
    .user_conn()
    .execute(UPDATE_CODE_QUERY, params!(email_verification_code))
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
