use axum::{
  extract::{Query, State},
  response::Redirect,
};
use lazy_static::lazy_static;
use serde::Deserialize;
use trailbase_sqlite::named_params;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::auth::password::{check_user_password, hash_password, validate_password_policy};
use crate::auth::ui::PROFILE_UI;
use crate::auth::util::validate_redirect;
use crate::auth::{AuthError, User};
use crate::constants::USER_TABLE;
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
  tag = "auth",
  params(ChangePasswordQuery),
  request_body = ChangePasswordRequest,
  responses(
    (status = 200, description = "Success.")
  )
)]
pub async fn change_password_handler(
  State(state): State<AppState>,
  Query(ChangePasswordQuery { redirect_to }): Query<ChangePasswordQuery>,
  user: User,
  either_request: Either<ChangePasswordRequest>,
) -> Result<Redirect, AuthError> {
  validate_redirect(&state, redirect_to.as_deref())?;

  let request = match either_request {
    Either::Json(req) => req,
    Either::Multipart(req, _) => req,
    Either::Form(req) => req,
  };

  let auth_options = state.auth_options();
  validate_password_policy(
    &request.new_password,
    &request.new_password_repeat,
    auth_options.password_options(),
  )?;

  let db_user = user_by_id(&state, &user.uuid).await?;

  // Validate old password.
  check_user_password(&db_user, &request.old_password, state.demo_mode())?;

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
      &*QUERY,
      named_params! {
        ":user_id": user.uuid.into_bytes().to_vec(),
        ":new_password_hash": new_password_hash,
        ":old_password_hash": old_password_hash,
      },
    )
    .await?;

  return match rows_affected {
    0 => Err(AuthError::BadRequest("Invalid old password")),
    1 => Ok(Redirect::to(redirect_to.as_deref().unwrap_or(PROFILE_UI))),
    _ => panic!("password changed for multiple users at once: {rows_affected}"),
  };
}
