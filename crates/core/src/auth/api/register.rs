use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use const_format::formatcp;
use serde::Deserialize;
use trailbase_sqlite::named_params;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::jwt::EmailVerificationTokenClaims;
use crate::auth::password::{hash_password, validate_password_policy};
use crate::auth::user::DbUser;
use crate::auth::util::{
  validate_and_normalize_email_address, validate_and_normalize_username, validate_redirect,
};
use crate::config::proto::UserIdentifier;
use crate::constants::USER_TABLE;
use crate::email::Email;
use crate::extract::Either;
use crate::util::urlencode;

#[derive(Debug, Default, Deserialize, IntoParams, ToSchema, TS)]
pub struct RegisterUserParams {
  redirect_uri: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema, TS)]
pub struct RegisterUserRequest {
  pub email: Option<String>,
  pub username: Option<String>,
  pub password: String,
  pub password_repeat: String,

  #[serde(flatten)]
  pub params: RegisterUserParams,
}

/// Registers a new user with email and password.
#[utoipa::path(
  post,
  path = "/register",
  tag = "auth",
  params(RegisterUserParams),
  request_body = RegisterUserRequest,
  responses(
    (status = 303, description = "Form fail OR success, new user registered, or user already exists."),
    (status = 424, description = "Failed to send verification Email."),
  )
)]
pub async fn register_user_handler(
  State(state): State<AppState>,
  Query(query): Query<RegisterUserParams>,
  either_request: Either<RegisterUserRequest>,
) -> Result<Response, AuthError> {
  let (disabled, user_identifier) = state.access_config(|c| {
    (
      c.auth.disable_password_auth.unwrap_or(false),
      c.auth.user_identifier,
    )
  });
  if disabled {
    return Err(AuthError::Forbidden);
  }

  let (request, json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let redirect_uri = validate_redirect(&state, query.redirect_uri.or(request.params.redirect_uri))?;
  let (normalized_email, username) = validate_email_and_username(
    user_identifier.and_then(|ui| ui.try_into().ok()),
    request.email.as_deref(),
    request.username.as_deref(),
  )?;

  let auth_options = state.auth_options();
  if let Err(err) = validate_password_policy(
    &request.password,
    &request.password_repeat,
    auth_options.password_options(),
  ) {
    if !json && let Some(redirect_uri) = redirect_uri {
      return Ok(
        Redirect::to(&format!(
          "{redirect_uri}?alert={msg}",
          msg = urlencode(&err.to_string()),
        ))
        .into_response(),
      );
    }

    return Err(err);
  }

  let success_response = {
    let normalized_email = normalized_email.clone();
    let redirect_uri = redirect_uri.clone();
    || {
      return match (redirect_uri, normalized_email) {
            (Some(ref redirect), Some(ref normalized_email)) => {
Redirect::to(&format!(
      "{redirect}?alert={msg}",
      msg = urlencode(&format!(
        "Registered {normalized_email}. Email verification is needed before signing in. Check your inbox."
      ))
    )).into_response()
            },
            (Some(ref redirect), None) => {
Redirect::to(redirect).into_response()
            },
            _ =>(StatusCode::OK, "registered").into_response(),
        };
    }
  };

  let hashed_password = hash_password(&request.password)?;

  const INSERT_USER_QUERY: &str = formatcp!(
    "\
      INSERT INTO \"{USER_TABLE}\" \
        (email, username, password_hash) \
      VALUES \
        (:email, :username, :password_hash) \
      RETURNING * \
    "
  );

  let user = match state
    .user_conn()
    .write_query_value::<DbUser>(
      INSERT_USER_QUERY,
      named_params! {
        ":email": normalized_email.clone(),
        ":username": username,
        ":password_hash": hashed_password,
      },
    )
    .await
  {
    Ok(Some(user)) => user,
    Err(_err) => {
      #[cfg(debug_assertions)]
      log::debug!("Failed to register new user {normalized_email:?}: {_err:?}");

      // In case the user already exists, we claim success to avoid leaking users' email addresses.
      return Ok(success_response());
    }
    Ok(None) => {
      return Err(AuthError::Internal("Failed to get user".into()));
    }
  };

  if let Some(ref email) = user.email {
    let claims =
      EmailVerificationTokenClaims::new(&user.uuid(), email.clone(), chrono::Duration::hours(4));
    let token = state
      .jwt()
      .encode(&claims)
      .map_err(|err| AuthError::Internal(err.into()))?;

    let email = Email::verification_email(&state, email, &token, redirect_uri.as_deref())
      .map_err(|err| AuthError::Internal(err.into()))?;
    email
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(format!("Failed to send Email {err}.").into()))?;
  }

  return Ok(success_response());
}

fn validate_email_and_username(
  user_identifier: Option<UserIdentifier>,
  email: Option<&str>,
  username: Option<&str>,
) -> Result<(Option<String>, Option<String>), AuthError> {
  return match user_identifier.unwrap_or(UserIdentifier::Undefined) {
    UserIdentifier::OnlyEmail | UserIdentifier::Undefined => match (email, username) {
      (_, u) if u.is_some_and(|u| !u.is_empty()) => {
        Err(AuthError::BadRequest("username not allowed"))
      }
      (None, _) | (Some(""), _) => Err(AuthError::BadRequest("Missing email")),
      (Some(email), _) => Ok((Some(validate_and_normalize_email_address(email)?), None)),
    },
    UserIdentifier::RequireEmail => match (email, username) {
      (None, _) | (Some(""), _) => Err(AuthError::BadRequest("Missing email")),
      (Some(email), None) | (Some(email), Some("")) => {
        Ok((Some(validate_and_normalize_email_address(email)?), None))
      }
      (Some(email), Some(username)) => Ok((
        Some(validate_and_normalize_email_address(email)?),
        Some(validate_and_normalize_username(username)?),
      )),
    },
    UserIdentifier::OnlyUsername => match (email, username) {
      (email, _) if email.is_some_and(|e| !e.is_empty()) => {
        Err(AuthError::BadRequest("Email not allowed"))
      }
      (_, None) | (_, Some("")) => Err(AuthError::BadRequest("Missing username")),
      (_, Some(username)) => Ok((None, Some(validate_and_normalize_username(username)?))),
    },
    UserIdentifier::RequireUsername => match (email, username) {
      (_, None) | (_, Some("")) => Err(AuthError::BadRequest("Missing username")),
      (None, Some(username)) | (Some(""), Some(username)) => {
        Ok((None, Some(validate_and_normalize_username(username)?)))
      }
      (Some(email), Some(username)) => Ok((
        Some(validate_and_normalize_email_address(email)?),
        Some(validate_and_normalize_username(username)?),
      )),
    },
    UserIdentifier::RequireEmailAndUsername => match (email, username) {
      (None, _) | (Some(""), _) => Err(AuthError::BadRequest("Missing Email")),
      (_, None) | (_, Some("")) => Err(AuthError::BadRequest("Missing username")),
      (Some(email), Some(username)) => Ok((
        Some(validate_and_normalize_email_address(email)?),
        Some(validate_and_normalize_username(username)?),
      )),
    },
  };
}
