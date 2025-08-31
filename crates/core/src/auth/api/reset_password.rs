use axum::{
  extract::State,
  response::{IntoResponse, Redirect, Response},
};
use lazy_static::lazy_static;
use serde::Deserialize;
use trailbase_sqlite::params;
use ts_rs::TS;
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::ui::LOGIN_UI;
use crate::constants::USER_TABLE;
use crate::email::Email;
use crate::extract::Either;
use crate::rand::generate_random_string;

use crate::auth::AuthError;
use crate::auth::password::{hash_password, validate_password_policy};
use crate::auth::util::{user_by_email, validate_and_normalize_email_address, validate_redirect};

const TTL_SEC: i64 = 3600;
const RATE_LIMIT_SEC: i64 = 4 * 3600;

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
pub struct ResetPasswordRequest {
  pub email: String,
}

/// Request a password reset.
#[utoipa::path(
  post,
  path = "/reset_password/request",
  tag = "auth",
  request_body = ResetPasswordRequest,
  responses(
    (status = 200, description = "Success.")
  )
)]
pub async fn reset_password_request_handler(
  State(state): State<AppState>,
  either_request: Either<ResetPasswordRequest>,
) -> Result<Response, AuthError> {
  let request = match either_request {
    Either::Json(req) => req,
    Either::Multipart(req, _) => req,
    Either::Form(req) => req,
  };

  let normalized_email = validate_and_normalize_email_address(&request.email)?;

  let user = user_by_email(&state, &normalized_email).await?;

  if let Some(last_reset) = user.password_reset_code_sent_at {
    let Some(timestamp) = chrono::DateTime::from_timestamp(last_reset, 0) else {
      return Err(AuthError::Internal("Invalid timestamp".into()));
    };

    let age: chrono::Duration = chrono::Utc::now() - timestamp;
    if age < chrono::Duration::seconds(RATE_LIMIT_SEC) {
      return Err(AuthError::BadRequest("Password reset sent already"));
    }
  }

  let password_reset_code = generate_random_string(20);
  lazy_static! {
    static ref UPDATE_CODE_QUERY: String = format!(
      r#"
          UPDATE
            '{USER_TABLE}'
          SET
            password_reset_code = $1,
            password_reset_code_sent_at = UNIXEPOCH()
          WHERE
            id = $2
        "#
    );
  }

  let rows_affected = state
    .user_conn()
    .execute(
      &*UPDATE_CODE_QUERY,
      params!(password_reset_code.clone(), user.id),
    )
    .await?;

  return match rows_affected {
    0 => Err(AuthError::Conflict),
    1 => {
      let email = Email::password_reset_email(&state, &user.email, &password_reset_code)
        .map_err(|err| AuthError::Internal(err.into()))?;
      email
        .send()
        .await
        .map_err(|err| AuthError::Internal(err.into()))?;

      Ok(Redirect::to(&format!("{LOGIN_UI}?alert=Password reset email sent")).into_response())
    }
    _ => {
      panic!("non-unique email");
    }
  };
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ResetPasswordUpdateRequest {
  pub password: String,
  pub password_repeat: String,

  pub password_reset_code: String,

  pub redirect_uri: Option<String>,
}

/// Endpoint for setting a new password after the user has requested a reset and provided a
/// replacement password.
#[utoipa::path(
  post,
  path = "/reset_password/update/:password_reset_code",
  tag = "auth",
  request_body = ResetPasswordUpdateRequest,
  responses(
    (status = 200, description = "Success.")
  )
)]
pub async fn reset_password_update_handler(
  State(state): State<AppState>,
  either_request: Either<ResetPasswordUpdateRequest>,
) -> Result<Response, AuthError> {
  let request = match either_request {
    Either::Json(req) => req,
    Either::Multipart(req, _) => req,
    Either::Form(req) => req,
  };

  validate_redirect(&state, request.redirect_uri.as_deref())?;

  let auth_options = state.auth_options();
  validate_password_policy(
    &request.password,
    &request.password_repeat,
    auth_options.password_options(),
  )?;

  let hashed_password = hash_password(&request.password)?;
  lazy_static! {
    static ref UPDATE_PASSWORD_QUERY: String = format!(
      r#"
        UPDATE '{USER_TABLE}'
        SET
          password_hash = $1,
          password_reset_code = NULL,
          password_reset_code_sent_at = NULL
        WHERE
          password_reset_code = $2 AND password_reset_code_sent_at > (UNIXEPOCH() - {TTL_SEC})
      "#
    );
  }

  let rows_affected = state
    .user_conn()
    .execute(
      &*UPDATE_PASSWORD_QUERY,
      params!(hashed_password, request.password_reset_code),
    )
    .await?;

  return match rows_affected {
    0 => Err(AuthError::BadRequest("Invalid reset code.")),
    1 => Ok(Redirect::to(request.redirect_uri.as_deref().unwrap_or(LOGIN_UI)).into_response()),
    _ => {
      panic!("multiple users with same verification code.");
    }
  };
}
