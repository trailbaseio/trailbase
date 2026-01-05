use axum::{
  extract::{Path, Query, State},
  http::StatusCode,
  response::{IntoResponse, Redirect, Response},
};
use const_format::formatcp;
use serde::Deserialize;
use trailbase_sqlite::params;
use utoipa::{IntoParams, ToSchema};

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::PROFILE_UI;
use crate::auth::util::{user_by_email, validate_redirect};
use crate::constants::{USER_TABLE, VERIFICATION_CODE_LENGTH};
use crate::email::Email;
use crate::rand::generate_random_string;

const TTL_SEC: i64 = 3600;
const RATE_LIMIT_SEC: i64 = 4 * 3600;

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct EmailVerificationRequest {
  pub email: String,
}

/// Request a new email to verify email address.
#[utoipa::path(
  get,
  path = "/verify_email/trigger",
  tag = "auth",
  request_body = EmailVerificationRequest,
  responses(
    (status = 200, description = "Email verification sent.")
  )
)]
pub async fn request_email_verification_handler(
  State(state): State<AppState>,
  Query(request): Query<EmailVerificationRequest>,
) -> Result<Response, AuthError> {
  let user = user_by_email(&state, &request.email).await?;

  if let Some(last_verification) = user.email_verification_code_sent_at {
    let Some(timestamp) = chrono::DateTime::from_timestamp(last_verification, 0) else {
      return Err(AuthError::Internal("Invalid timestamp".into()));
    };

    let age: chrono::Duration = chrono::Utc::now() - timestamp;
    if age < chrono::Duration::seconds(RATE_LIMIT_SEC) {
      return Err(AuthError::BadRequest("verification already sent"));
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
    0 => Err(AuthError::Conflict),
    1 => {
      let email = Email::verification_email(&state, &user.email, &email_verification_code)
        .map_err(|err| AuthError::Internal(err.into()))?;
      email
        .send()
        .await
        .map_err(|err| AuthError::Internal(err.into()))?;

      Ok((StatusCode::OK, "Verification code sent").into_response())
    }
    _ => {
      panic!("Password reset affected multiple users: {rows_affected}");
    }
  };
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub(crate) struct VerifyEmailQuery {
  pub redirect_uri: Option<String>,
}

/// Request a new email to verify email address.
#[utoipa::path(
  get,
  path = "/verify_email/confirm/:email_verification_code",
  tag = "auth",
  responses(
    (status = 200, description = "Email verified.")
  )
)]
pub async fn verify_email_handler(
  State(state): State<AppState>,
  Path(email_verification_code): Path<String>,
  Query(VerifyEmailQuery { redirect_uri }): Query<VerifyEmailQuery>,
) -> Result<Redirect, AuthError> {
  validate_redirect(&state, redirect_uri.as_deref())?;

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
    0 => Err(AuthError::BadRequest("Invalid verification code")),
    1 => Ok(Redirect::to(redirect_uri.as_deref().unwrap_or(PROFILE_UI))),
    _ => panic!("email verification affected multiple users: {rows_affected}"),
  };
}
