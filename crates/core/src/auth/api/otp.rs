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
use crate::auth::user::DbUser;
use crate::auth::util::{
  get_user_by_id, user_by_email, user_by_username, validate_and_normalize_email_address,
  validate_and_normalize_username, validate_redirect,
};
use crate::constants::OTP_CODE_TABLE;
use crate::email::Email;
use crate::extract::Either;
use crate::rand::random_numeric_and_uppercase;
use crate::util::urlencode;

#[derive(Debug, Default, Deserialize, IntoParams, ToSchema, TS)]
pub struct RequestOtpParams {
  pub redirect_uri: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum RequestOtpRequest {
  EmailOrUsername {
    email_or_username: String,

    #[serde(flatten)]
    params: RequestOtpParams,
  },
  Email {
    email: String,
    #[serde(flatten)]
    params: RequestOtpParams,
  },
  Username {
    username: String,
    #[serde(flatten)]
    params: RequestOtpParams,
  },
}

enum UserIdentifier {
  Email(String),
  Username(String),
}

#[utoipa::path(
  post,
  path = "/otp/request",
  tag = "auth",
  params(RequestOtpParams),
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
  Query(query): Query<RequestOtpParams>,
  either_request: Either<RequestOtpRequest>,
) -> Result<Response, AuthError> {
  if !state.access_config(|c| c.auth.enable_otp_signin()) {
    return Err(AuthError::MethodNotAllowed);
  }

  let (request, _json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let (user_identifier, params) = match request {
    RequestOtpRequest::EmailOrUsername {
      email_or_username,
      params,
    } => {
      if email_or_username.contains('@') {
        (
          UserIdentifier::Email(validate_and_normalize_email_address(&email_or_username)?),
          params,
        )
      } else {
        (
          UserIdentifier::Username(validate_and_normalize_username(&email_or_username)?),
          params,
        )
      }
    }
    RequestOtpRequest::Email { email, params } => (UserIdentifier::Email(email), params),
    RequestOtpRequest::Username { username, params } => {
      (UserIdentifier::Username(username), params)
    }
  };

  let (user, redirect_uri, success_response): (
    DbUser,
    Option<String>,
    Box<dyn FnOnce() -> Response + Send>,
  ) = match user_identifier {
    UserIdentifier::Email(email) => {
      let redirect_uri = validate_redirect(&state, query.redirect_uri.or(params.redirect_uri))?;
      let normalized_email = validate_and_normalize_email_address(&email)?;

      rate_limit_otp_requests(normalized_email.clone())?;

      let success_response = {
        let redirect_uri = redirect_uri.clone();
        let email = normalized_email.clone();
        move || success_response_impl(redirect_uri.as_deref(), Some(&email), None)
      };

      // We need to check the email is associated with actual user to not just send emails to
      // anyone.
      let Ok(user) = user_by_email(&state, &normalized_email).await else {
        // In case we don't find a user we still reply with a success to avoid leaking
        // users' email addresses.
        return Ok(success_response());
      };

      debug_assert_eq!(Some(&normalized_email), user.email.as_ref());

      (user, redirect_uri, Box::new(success_response))
    }
    UserIdentifier::Username(username) => {
      let redirect_uri = validate_redirect(&state, query.redirect_uri.or(params.redirect_uri))?;
      let normalized_username = validate_and_normalize_username(&username)?;

      rate_limit_otp_requests(normalized_username.clone())?;

      let success_response = {
        let redirect_uri = redirect_uri.clone();
        let username = normalized_username.clone();
        move || success_response_impl(redirect_uri.as_deref(), None, Some(&username))
      };

      let Ok(user) = user_by_username(&state, &normalized_username).await else {
        // In case we don't find a user we still reply with a success to avoid leaking
        // users' username.
        return Ok(success_response());
      };

      (user, redirect_uri, Box::new(success_response))
    }
  };

  let Some(ref normalized_email) = user.email else {
    // In case the user doesn't have an email address, there's no way to send an OTP code right
    // now. Reply with success to avoid leaking this fact.
    return Ok(success_response());
  };

  if user.totp_secret.is_some() {
    // If the user has two/multi-factor-auth enabled, allowing OTP-only login would be a break of
    // contract. We may want to support OTP + TOTP going forward.
    #[cfg(debug_assertions)]
    log::debug!("Skipping OTP request for user with two-factor auth enabled.");

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
        user.id,
        normalized_email.clone(),
        otp_code.clone(),
        (Utc::now() + OTP_TTL).timestamp(),
      ),
    )
    .await?;

  if rows_affected != 1 {
    return Err(AuthError::Internal("Failed to insert OTP code".into()));
  }

  let email = Email::otp_email(&state, normalized_email, &otp_code, redirect_uri.as_deref())
    .map_err(|err| AuthError::Internal(err.into()))?;
  email
    .send()
    .await
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(success_response());
}

#[derive(Debug, Default, Deserialize, IntoParams, ToSchema, TS)]
pub struct LoginOtpParams {
  pub email: Option<String>,
  pub username: Option<String>,
  pub code: Option<String>,
  pub redirect_uri: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema, TS)]
#[ts(export)]
pub struct LoginOtpRequest {
  #[serde(flatten)]
  pub params: LoginOtpParams,
}

#[utoipa::path(
  post,
  path = "/otp/login",
  tag = "auth",
  params(LoginOtpParams),
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
  Query(query): Query<LoginOtpParams>,
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

  let LoginOtpRequest { params } = request;

  let redirect_uri = validate_redirect(&state, query.redirect_uri.or(params.redirect_uri))?;
  let Some(otp_code) = query.code.as_deref().or(params.code.as_deref()) else {
    // TODO: Add redirect for non-json
    return Err(AuthError::BadRequest("missing code"));
  };

  let email = query.email.as_deref().or(params.email.as_deref());
  let username = query.username.as_deref().or(params.username.as_deref());

  let user_id = match (email, username) {
    (Some(email), _) => {
      let normalized_email = validate_and_normalize_email_address(email)?;

      rate_limit_otp_logins(normalized_email.clone())?;

      const LOOKUP_OTP_QUERY: &str = formatcp!(
        "\
          SELECT user FROM '{OTP_CODE_TABLE}' \
          WHERE  \
            email = $1 AND \
            otp_code = $2 AND \
            UNIXEPOCH() < expires \
        "
      );

      state
        .session_conn()
        .read_query_row_get(
          LOOKUP_OTP_QUERY,
          params!(normalized_email, otp_code.trim().to_string()),
          0,
        )
        .await?
        .ok_or(AuthError::Unauthorized)?
    }

    (None, Some(username)) => {
      let normalized_username = validate_and_normalize_username(username)?;

      rate_limit_otp_logins(normalized_username.clone())?;

      let Ok(DbUser {
        email: Some(email), ..
      }) = user_by_username(&state, &normalized_username).await
      else {
        return Err(AuthError::Unauthorized);
      };

      const LOOKUP_OTP_QUERY: &str = formatcp!(
        "\
          SELECT user FROM '{OTP_CODE_TABLE}' \
          WHERE  \
            email = $1 AND \
            otp_code = $2 AND \
            UNIXEPOCH() < expires \
        "
      );

      state
        .session_conn()
        .read_query_row_get(
          LOOKUP_OTP_QUERY,
          params!(email, otp_code.trim().to_string()),
          0,
        )
        .await?
        .ok_or(AuthError::Unauthorized)?
    }
    _ => {
      return Err(AuthError::BadRequest("missing email or username"));
    }
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

fn success_response_impl(
  redirect_uri: Option<&str>,
  normalized_email: Option<&str>,
  normalized_username: Option<&str>,
) -> Response {
  const MSG: &str = "OTP sent";
  return match (redirect_uri, normalized_email, normalized_username) {
    (Some(redirect), Some(email), _) => Redirect::to(&format!(
      "{redirect}?email={email}&alert={msg}",
      msg = urlencode(MSG)
    ))
    .into_response(),
    (Some(redirect), None, Some(username)) => Redirect::to(&format!(
      "{redirect}?username={username}&alert={msg}",
      msg = urlencode(MSG)
    ))
    .into_response(),
    _ => (StatusCode::OK, "OTP sent").into_response(),
  };
}

const OTP_CODE_LENGTH: usize = 6;
const OTP_TTL: Duration = Duration::minutes(5);

// Track attempts to request OTP codes for abuse prevention.
fn rate_limit_otp_requests(id: String) -> Result<(), AuthError> {
  static REQUEST_ATTEMPTS: LazyLock<Cache<String, ()>> = LazyLock::new(|| {
    Cache::builder()
      .time_to_live(std::time::Duration::from_secs(OTP_TTL.num_seconds() as u64))
      .max_capacity(2048)
      .build()
  });

  if REQUEST_ATTEMPTS.get(&id).is_some() {
    return Err(AuthError::TooManyRequests);
  }

  REQUEST_ATTEMPTS.insert(id, ());

  return Ok(());
}

// Track login attempts for abuse prevention.
fn rate_limit_otp_logins(id: String) -> Result<(), AuthError> {
  static LOGIN_ATTEMPTS: LazyLock<Cache<String, usize>> = LazyLock::new(|| {
    Cache::builder()
      .time_to_live(std::time::Duration::from_secs(60))
      .max_capacity(2048)
      .build()
  });

  if let Some(attempts) = LOGIN_ATTEMPTS.get(&id) {
    if attempts >= 3 {
      return Err(AuthError::TooManyRequests);
    }
    LOGIN_ATTEMPTS.insert(id, attempts + 1);
  } else {
    LOGIN_ATTEMPTS.insert(id, 1);
  }
  return Ok(());
}
