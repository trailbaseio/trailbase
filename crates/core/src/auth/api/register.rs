use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use const_format::formatcp;
use serde::Deserialize;
use trailbase_sqlite::named_params;
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::password::{hash_password, validate_password_policy};
use crate::auth::user::DbUser;
use crate::auth::util::{user_exists, validate_and_normalize_email_address};
use crate::auth::{LOGIN_UI, REGISTER_USER_UI};
use crate::constants::{USER_TABLE, VERIFICATION_CODE_LENGTH};
use crate::email::Email;
use crate::extract::Either;
use crate::rand::generate_random_string;

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
  tag = "auth",
  request_body = RegisterUserRequest,
  responses(
    (status = 200, description = "Successful registration.")
  )
)]
pub async fn register_user_handler(
  State(state): State<AppState>,
  either_request: Either<RegisterUserRequest>,
) -> Result<Response, AuthError> {
  let disabled = state.access_config(|c| c.auth.disable_password_auth.unwrap_or(false));
  if disabled {
    return Err(AuthError::Forbidden);
  }

  let request = match either_request {
    Either::Json(req) => req,
    Either::Multipart(req, _) => req,
    Either::Form(req) => req,
  };

  let normalized_email = validate_and_normalize_email_address(&request.email)?;

  let auth_options = state.auth_options();
  if let Err(_err) = validate_password_policy(
    &request.password,
    &request.password_repeat,
    auth_options.password_options(),
  ) {
    let msg = crate::util::urlencode("Invalid password");
    return Ok(Redirect::to(&format!("{REGISTER_USER_UI}?alert={msg}")).into_response());
  }

  let exists = user_exists(&state, &normalized_email).await?;
  if exists {
    let msg = crate::util::urlencode("E-mail already registered.");
    return Ok(Redirect::to(&format!("{REGISTER_USER_UI}?alert={msg}")).into_response());
  }

  let email_verification_code = generate_random_string(VERIFICATION_CODE_LENGTH);
  let hashed_password = hash_password(&request.password)?;

  const INSERT_USER_QUERY: &str = formatcp!(
    " \
      INSERT INTO '{USER_TABLE}' \
        (email, password_hash, email_verification_code, email_verification_code_sent_at) \
      VALUES \
        (:email, :password_hash, :email_verification_code, UNIXEPOCH()) \
      RETURNING * \
    "
  );

  let Some(user) = state
    .user_conn()
    .write_query_value::<DbUser>(
      INSERT_USER_QUERY,
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

  let email = Email::verification_email(&state, &user.email, &email_verification_code)
    .map_err(|err| AuthError::Internal(err.into()))?;
  email
    .send()
    .await
    .map_err(|err| AuthError::Internal(err.into()))?;

  // Success: new user registered. User still needs to verify.
  let msg = format!(
    "Registered {normalized_email}. Email verification is needed before sign in. Check your inbox."
  );
  return Ok(Redirect::to(&format!("{LOGIN_UI}?alert={msg}")).into_response());
}
