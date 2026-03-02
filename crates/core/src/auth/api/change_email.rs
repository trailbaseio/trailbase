use axum::{
  extract::{OriginalUri, Path, Query, State},
  http::StatusCode,
  response::{IntoResponse, Redirect, Response},
};
use const_format::formatcp;
use serde::Deserialize;
use trailbase_sqlite::params;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::app_state::AppState;
use crate::auth::jwt::EmailChangeTokenClaims;
use crate::auth::util::{user_by_id, validate_and_normalize_email_address, validate_redirect};
use crate::auth::{AuthError, User};
use crate::constants::USER_TABLE;
use crate::email::Email;
use crate::extract::Either;
use crate::util::urlencode;

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ChangeEmailQuery {
  redirect_uri: Option<String>,
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct ChangeEmailRequest {
  pub csrf_token: String,
  pub old_email: Option<String>,
  pub new_email: String,

  pub redirect_uri: Option<String>,
}

/// Request an email change.
#[utoipa::path(
  post,
  path = "/change_email/request",
  tag = "auth",
  params(ChangeEmailQuery),
  request_body = ChangeEmailRequest,
  responses(
    (status = 200, description = "Success, when redirect_uri is not present and JSON input"),
    (status = 303, description = "Success, when redirect_uri is present or HTML form input"),
    (status = 400, description = "Bad request."),
    (status = 403, description = "User conflict."),
    (status = 429, description = "Too many attempts."),
  )
)]
pub async fn change_email_request_handler(
  State(state): State<AppState>,
  origin: OriginalUri,
  user: User,
  query: Query<ChangeEmailQuery>,
  either_request: Either<ChangeEmailRequest>,
) -> Result<Response, AuthError> {
  let (request, json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let redirect_uri = validate_redirect(
    &state,
    query
      .redirect_uri
      .as_deref()
      .or(request.redirect_uri.as_deref()),
  )?;

  if request.csrf_token != user.csrf_token {
    return Err(AuthError::BadRequest("Invalid CSRF token"));
  }

  let new_email = validate_and_normalize_email_address(&request.new_email)?;
  let Ok(db_user) = user_by_id(&state, &user.uuid).await else {
    return Err(AuthError::Forbidden);
  };

  // NOTE: This is pretty arbitrary, we could do away with this entirely :shrug:.
  if !json {
    let Some(ref old_email) = request.old_email else {
      return Err(AuthError::BadRequest("Missing old email address"));
    };

    if validate_and_normalize_email_address(old_email)? != db_user.email {
      return Err(AuthError::BadRequest("Mismatch: old email address"));
    }
  }

  let claims = EmailChangeTokenClaims::new(
    &db_user.uuid(),
    db_user.email.clone(),
    new_email,
    chrono::Duration::hours(4),
  );
  let token = state
    .jwt()
    .encode(&claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  let email = Email::change_email_address_email(&state, &db_user.email, &token)
    .map_err(|err| AuthError::Internal(err.into()))?;
  email
    .send()
    .await
    .map_err(|err| AuthError::Internal(err.into()))?;

  if let Some(ref redirect) = redirect_uri {
    return Ok(
      Redirect::to(&format!(
        "{redirect}?alert={msg}",
        msg = urlencode("Verification email sent")
      ))
      .into_response(),
    );
  }

  // Fallback
  if !json {
    return Ok(
      Redirect::to(&format!(
        "{path}?alert={msg}",
        path = origin.path(),
        msg = urlencode("Verification email sent")
      ))
      .into_response(),
    );
  }

  return Ok((StatusCode::OK, "Verification email sent.").into_response());
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub(crate) struct ChangeEmailConfigQuery {
  pub redirect_uri: Option<String>,
}

/// Confirm a change of email address.
#[utoipa::path(
  get,
  path = "/change_email/confirm/:email_verification_code",
  tag = "auth",
  responses(
    (status = 200, description = "Success, when redirect_uri is not present."),
    (status = 303, description = "Success, when redirect_uri is present."),
  )
)]
pub async fn change_email_confirm_handler(
  State(state): State<AppState>,
  Path(email_verification_token): Path<String>,
  query: Query<ChangeEmailConfigQuery>,
  // user: Option<User>,
) -> Result<Response, AuthError> {
  if state.demo_mode() {
    return Err(AuthError::BadRequest("Disallowed in demo"));
  }

  let redirect_uri = validate_redirect(&state, query.redirect_uri.as_deref())?;
  let claims = EmailChangeTokenClaims::decode(state.jwt(), &email_verification_token)
    .map_err(|_err| AuthError::BadRequest("Invalid token"))?;

  const QUERY: &str = formatcp!(
    "\
      UPDATE '{USER_TABLE}' \
      SET \
        email = $1, \
        verified = TRUE \
      WHERE \
        email = $2 \
    "
  );

  let rows_affected = state
    .user_conn()
    .execute(QUERY, params!(claims.new_email, claims.old_email))
    .await?;

  return match rows_affected {
    0 => Err(AuthError::Conflict),
    1 => {
      if let Some(redirect) = redirect_uri {
        Ok(Redirect::to(redirect).into_response())
      } else if state.public_dir().is_some() {
        Ok(Redirect::to("/").into_response())
      } else {
        Ok((StatusCode::OK, "email changed").into_response())
      }
    }
    _ => panic!("emails updated for multiple users at once: {rows_affected}"),
  };
}
