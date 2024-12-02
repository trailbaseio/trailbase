use axum::{
  extract::{Form, State},
  http::StatusCode,
  response::{IntoResponse, Redirect, Response},
};
use lazy_static::lazy_static;
use serde::Deserialize;
use tokio_rusqlite::named_params;
use utoipa::ToSchema;
use validator::ValidateEmail;

use crate::app_state::AppState;
use crate::auth::password::{hash_password, validate_passwords};
use crate::auth::user::DbUser;
use crate::auth::util::user_exists;
use crate::auth::AuthError;
use crate::constants::{PASSWORD_OPTIONS, USER_TABLE, VERIFICATION_CODE_LENGTH};
use crate::email::Email;
use crate::rand::generate_random_string;

/// Validates the given email addresses and returns a best-effort normalized address.
///
/// NOTE: That there's no robust way to detect equivalent addresses, default mappings are highly
/// domain specific, e.g. most mail providers will treat emails as case insensitive and others have
/// custom rules such as gmail stripping all "." and everything after and including "+". Trying to
/// be overly smart is probably a recipe for disaster.
pub fn validate_and_normalize_email_address(address: &str) -> Result<String, AuthError> {
  if !address.validate_email() {
    return Err(AuthError::BadRequest("Invalid email"));
  }

  // TODO: detect and reject one-time burner email addresses.

  return Ok(address.to_ascii_lowercase());
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct RegisterUserRequest {
  pub email: String,
  pub password: String,
  pub password_repeat: String,
}

/// Registers a new user with email and password.
#[utoipa::path(
  post,
  path = "/register",
  request_body = RegisterUserRequest,
  responses(
    (status = 200, description = "Successful registration.")
  )
)]
pub async fn register_user_handler(
  State(state): State<AppState>,
  Form(request): Form<RegisterUserRequest>,
) -> Result<Response, AuthError> {
  let normalized_email = validate_and_normalize_email_address(&request.email)?;

  if let Err(_err) = validate_passwords(
    &request.password,
    &request.password_repeat,
    &PASSWORD_OPTIONS,
  ) {
    let msg = crate::util::urlencode("Invalid password");
    return Ok(Redirect::to(&format!("/_/auth/register/?alert={msg}")).into_response());
  }

  let exists = user_exists(&state, &normalized_email).await?;
  if exists {
    let msg = crate::util::urlencode("E-mail already registered.");
    return Ok(Redirect::to(&format!("/_/auth/register/?alert={msg}")).into_response());
  }

  let email_verification_code = generate_random_string(VERIFICATION_CODE_LENGTH);
  let hashed_password = hash_password(&request.password)?;

  lazy_static! {
    static ref INSERT_USER_QUERY: String = format!(
      r#"
        INSERT INTO '{USER_TABLE}'
          (email, password_hash, email_verification_code, email_verification_code_sent_at)
        VALUES
          (:email, :password_hash, :email_verification_code, UNIXEPOCH())
        RETURNING *
      "#
    );
  }

  let Some(user) = state
    .user_conn()
    .query_value::<DbUser>(
      &INSERT_USER_QUERY,
      named_params! {
        ":email": normalized_email.clone(),
        ":password_hash": hashed_password,
        ":email_verification_code": email_verification_code.clone(),
      },
    )
    .await
    .map_err(|_err| {
      #[cfg(debug_assertions)]
      log::debug!("Failed to create user {normalized_email}: {_err}");
      // The insert will fail if the user is already registered
      AuthError::Conflict
    })?
  else {
    return Err(AuthError::Internal("Failed to get user".into()));
  };

  let email = Email::verification_email(&state, &user, &email_verification_code)
    .map_err(|err| AuthError::Internal(err.into()))?;
  email
    .send()
    .await
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok((StatusCode::OK, "User registered").into_response());
}
