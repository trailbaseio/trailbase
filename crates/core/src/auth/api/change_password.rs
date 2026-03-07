use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use const_format::formatcp;
use serde::Deserialize;
use trailbase_sqlite::named_params;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::auth::password::{check_user_password, hash_password, validate_password_policy};
use crate::auth::util::validate_redirect;
use crate::auth::{AuthError, User};
use crate::constants::USER_TABLE;
use crate::extract::Either;
use crate::util::urlencode;
use crate::{app_state::AppState, auth::util::user_by_id};

#[derive(Debug, Default, Deserialize, IntoParams)]
pub(crate) struct ChangePasswordQuery {
  /// Success (and error if err_redirect_uri not present) redirect target for non-JSON requests.
  pub redirect_uri: Option<String>,
  /// Error redirect target for non-JSON requests.
  pub err_redirect_uri: Option<String>,
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct ChangePasswordRequest {
  pub old_password: String,
  pub new_password: String,
  pub new_password_repeat: String,

  /// Success (and error if err_redirect_uri not present) redirect target for non-JSON requests.
  pub redirect_uri: Option<String>,
  /// Error redirect target for non-JSON requests.
  pub err_redirect_uri: Option<String>,
}

/// Request a change of password.
#[utoipa::path(
  post,
  path = "/change_password",
  tag = "auth",
  params(ChangePasswordQuery),
  request_body = ChangePasswordRequest,
  responses(
    (status = 200, description = "Success, when redirect_uri not present."),
    (status = 303, description = "Success, when redirect_uri present."),
  )
)]
pub async fn change_password_handler(
  State(state): State<AppState>,
  Query(query): Query<ChangePasswordQuery>,
  user: User,
  either_request: Either<ChangePasswordRequest>,
) -> Result<Response, AuthError> {
  if state.demo_mode() {
    return Err(AuthError::BadRequest("Disallowed in demo"));
  }

  let (request, json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let redirect_uri = validate_redirect(&state, query.redirect_uri.or(request.redirect_uri))?;
  let err_redirect_uri =
    validate_redirect(&state, query.err_redirect_uri.or(request.err_redirect_uri))?;

  if let Err(err) = validate_password_policy(
    &request.new_password,
    &request.new_password_repeat,
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

  // Validate old password.
  //
  // TODO: It would probably be good practice to check TOTP as well for users of multi-factor auth.
  if let Err(_err) = check_user_password(&db_user, &request.old_password, state.demo_mode()) {
    const MSG: &str = "invalid `old_password`";
    if !json && let Some(redirect_uri) = err_redirect_uri.or(redirect_uri) {
      return Ok(
        Redirect::to(&format!("{redirect_uri}?alert={msg}", msg = urlencode(MSG))).into_response(),
      );
    }
    return Err(AuthError::BadRequest(MSG));
  };

  // NOTE: we're using the old_password_hash to prevent races between concurrent change requests
  // for the same user.
  let old_password_hash = db_user.password_hash;
  let new_password_hash = hash_password(&request.new_password)?;

  const QUERY: &str = formatcp!(
    "\
      UPDATE '{USER_TABLE}' \
      SET password_hash = :new_password_hash \
      WHERE id = :user_id AND password_hash = :old_password_hash \
    "
  );

  let rows_affected = state
    .user_conn()
    .execute(
      QUERY,
      named_params! {
        ":user_id": user.uuid.into_bytes().to_vec(),
        ":new_password_hash": new_password_hash,
        ":old_password_hash": old_password_hash,
      },
    )
    .await?;

  return match rows_affected {
    0 => Err(AuthError::BadRequest("Invalid old password")),
    1 => {
      if let Some(ref redirect) = redirect_uri {
        Ok(
          Redirect::to(&format!(
            "{redirect}?alert={msg}",
            msg = urlencode("password changed")
          ))
          .into_response(),
        )
      } else {
        Ok((StatusCode::OK, "password changed").into_response())
      }
    }
    _ => panic!("password changed for multiple users at once: {rows_affected}"),
  };
}
