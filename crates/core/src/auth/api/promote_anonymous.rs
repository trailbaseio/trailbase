use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use const_format::formatcp;
use serde::Deserialize;
use trailbase_sqlite::named_params;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::app_state::AppState;
use crate::auth::jwt::EmailVerificationTokenClaims;
use crate::auth::password::{hash_password, validate_password_policy};
use crate::auth::util::{user_by_id, validate_redirect};
use crate::auth::{AuthError, User};
use crate::constants::USER_TABLE;
use crate::email::Email;
use crate::extract::Either;
use crate::util::urlencode;

#[derive(Debug, Default, Deserialize, IntoParams, ToSchema, TS)]
pub(crate) struct PromoteAnonymousParams {
  /// Success (and error if err_redirect_uri not present) redirect target for non-JSON requests.
  pub redirect_uri: Option<String>,
  /// Error redirect target for non-JSON requests.
  pub err_redirect_uri: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema, TS)]
#[ts(export)]
pub struct PromoteAnonymousRequest {
  pub new_password: String,
  pub new_password_repeat: Option<String>,

  pub new_username: Option<String>,
  pub new_email: Option<String>,

  #[serde(flatten)]
  pub params: PromoteAnonymousParams,
}

/// Request a change of password.
#[utoipa::path(
  post,
  path = "/promote_anonymous",
  tag = "auth",
  params(PromoteAnonymousParams),
  request_body = PromoteAnonymousRequest,
  responses(
    (status = 200, description = "Success, when redirect_uri not present."),
    (status = 303, description = "Success, when redirect_uri present."),
  )
)]
pub async fn promote_anonymous_user_handler(
  State(state): State<AppState>,
  Query(query): Query<PromoteAnonymousParams>,
  user: User,
  either_request: Either<PromoteAnonymousRequest>,
) -> Result<Response, AuthError> {
  if state.demo_mode() {
    return Err(AuthError::BadRequest("Disallowed in demo"));
  }

  let (request, json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let redirect_uri = validate_redirect(&state, query.redirect_uri.or(request.params.redirect_uri))?;
  let err_redirect_uri = validate_redirect(
    &state,
    query.err_redirect_uri.or(request.params.err_redirect_uri),
  )?;

  let user_identifier = state
    .access_config(|c| c.auth.user_identifier)
    .and_then(|ui| ui.try_into().ok());

  let (normalized_email, username) = crate::auth::api::register::validate_email_and_username(
    user_identifier,
    request.new_email.as_deref(),
    request.new_username.as_deref(),
  )?;

  if let Err(err) = validate_password_policy(
    &request.new_password,
    request
      .new_password_repeat
      .as_ref()
      .unwrap_or(&request.new_password),
    state.auth_options().password_options(),
  ) {
    if !json && let Some(redirect_uri) = err_redirect_uri.or(redirect_uri) {
      return Ok(
        Redirect::to(&format!(
          "{redirect_uri}?alert={msg}",
          msg = urlencode(&err.to_string())
        ))
        .into_response(),
      );
    }
    return Err(err);
  }

  let db_user = user_by_id(&state, &user.uuid).await?;
  if db_user.password_hash.is_some() || db_user.provider_id > 0 {
    return Err(AuthError::FailedDependency("not an anonymous user".into()));
  }

  // NOTE: we're using the old_password_hash to prevent races between concurrent change requests
  // for the same user.
  let new_password_hash = hash_password(&request.new_password)?;

  const QUERY: &str = formatcp!(
    "\
      UPDATE \"{USER_TABLE}\" \
      SET \
        password_hash = :new_password_hash, \
        email = :email,
        verified = FALSE,
        username = :username
      WHERE id = :user_id \
    "
  );

  let rows_affected = state
    .user_conn()
    .execute(
      QUERY,
      named_params! {
        ":user_id": user.uuid.into_bytes().to_vec(),
        ":new_password_hash": new_password_hash,
        ":email": normalized_email.clone(),
        ":username": username,
      },
    )
    .await?;

  return match rows_affected {
    0 => Err(AuthError::BadRequest("Invalid old password")),
    1 => {
      if let Some(ref email) = normalized_email {
        let claims =
          EmailVerificationTokenClaims::new(&user.uuid, email.clone(), chrono::Duration::hours(4));
        let token = state
          .jwt()
          .encode(&claims)
          .map_err(|err| AuthError::Internal(err.into()))?;

        let email = Email::verification_email(&state, email, &token, redirect_uri.as_deref())
          .map_err(|err| AuthError::Internal(err.into()))?;

        email
          .send()
          .await
          .map_err(|err| AuthError::Internal(format!("Failed to send Email {err}.").into()))?;
      }

      if let Some(ref redirect) = redirect_uri {
        Ok(
          Redirect::to(&format!(
            "{redirect}?alert={msg}",
            msg = urlencode("promoted")
          ))
          .into_response(),
        )
      } else {
        Ok((StatusCode::OK, "promoted").into_response())
      }
    }
    _ => panic!("password changed for multiple users at once: {rows_affected}"),
  };
}
