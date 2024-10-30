use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
  extract::{Query, State},
  response::Redirect,
};
use lazy_static::lazy_static;
use libsql::named_params;
use serde::Deserialize;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::auth::password::{hash_password, validate_passwords};
use crate::auth::util::validate_redirects;
use crate::auth::{AuthError, User};
use crate::constants::{PASSWORD_OPTIONS, USER_TABLE};
use crate::extract::Either;
use crate::{app_state::AppState, auth::util::user_by_id};

#[derive(Debug, Default, Deserialize, IntoParams)]
pub(crate) struct ChangePasswordQuery {
  pub redirect_to: Option<String>,
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct ChangePasswordRequest {
  pub old_password: String,
  pub new_password: String,
  pub new_password_repeat: String,
}

/// Request a change of password.
#[utoipa::path(
  post,
  path = "/change_password",
  params(ChangePasswordQuery),
  request_body = ChangePasswordRequest,
  responses(
    (status = 200, description = "Success.")
  )
)]
pub async fn change_password_handler(
  State(state): State<AppState>,
  Query(query): Query<ChangePasswordQuery>,
  user: User,
  either_request: Either<ChangePasswordRequest>,
) -> Result<Redirect, AuthError> {
  let redirect = validate_redirects(&state, &query.redirect_to, &None)?;

  let request = match either_request {
    Either::Json(req) => req,
    Either::Multipart(req, _) => req,
    Either::Form(req) => req,
  };

  validate_passwords(
    &request.new_password,
    &request.new_password_repeat,
    &PASSWORD_OPTIONS,
  )?;

  let db_user = user_by_id(&state, &user.uuid).await?;

  // Validate old password.
  let parsed_hash = PasswordHash::new(&db_user.password_hash)
    .map_err(|err| AuthError::Internal(err.to_string().into()))?;
  Argon2::default()
    .verify_password(request.old_password.as_bytes(), &parsed_hash)
    .map_err(|_err| AuthError::Unauthorized)?;

  // NOTE: we're using the old_password_hash to prevent races between concurrent change requests
  // for the same user.
  let old_password_hash = db_user.password_hash;
  let new_password_hash = hash_password(&request.new_password)?;

  lazy_static! {
    pub static ref QUERY: String = format!(
      r#"
        UPDATE
          '{USER_TABLE}'
        SET
          password_hash = :new_password_hash
        WHERE
          id = :user_id AND password_hash = :old_password_hash
      "#
    );
  }

  let rows_affected = state
    .user_conn()
    .execute(
      &QUERY,
      named_params! {
        ":user_id": user.uuid.into_bytes(),
        ":new_password_hash": new_password_hash,
        ":old_password_hash": old_password_hash,
      },
    )
    .await?;

  return match rows_affected {
    0 => Err(AuthError::BadRequest("Invalid old password")),
    1 => Ok(Redirect::to(
      redirect.as_deref().unwrap_or("/_/auth/profile/"),
    )),
    _ => panic!("password changed for multiple users at once: {rows_affected}"),
  };
}
