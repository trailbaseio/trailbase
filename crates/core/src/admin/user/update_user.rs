use axum::{
  Json,
  extract::State,
  http::StatusCode,
  response::{IntoResponse, Response},
};
use const_format::formatcp;
// use rusqlite::params;
use serde::{Deserialize, Serialize};
use trailbase_sqlite::{Value, named_params};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::auth::password::hash_password;
use crate::auth::util::is_admin;
use crate::auth::util::{validate_and_normalize_email_address, validate_and_normalize_username};
use crate::constants::USER_TABLE;

/// Request changes to user with given `id`.
///
/// NOTE: We don't allow admin promotions and especially demotions, since they could easily be
/// abused. Instead we relegate such critical actions to the CLI, which limits them to sys
/// admins over mere TrailBase admins.
#[derive(Debug, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct UpdateUserRequest {
  id: uuid::Uuid,

  email: Option<String>,
  unverified_email: Option<String>,
  username: Option<String>,

  password: Option<String>,
}

pub async fn update_user_handler(
  State(state): State<AppState>,
  Json(request): Json<UpdateUserRequest>,
) -> Result<Response, Error> {
  let UpdateUserRequest {
    id: user_id,
    email,
    unverified_email,
    username,
    password,
  } = request;

  if is_admin(&state, &user_id).await {
    return Err(Error::Precondition(
      "Admins can only be updated using the CLI to prevent abuse".into(),
    ));
  }

  fn validate_email(email: String) -> Result<String, Error> {
    if email.is_empty() {
      return Ok(email);
    }
    return Ok(validate_and_normalize_email_address(&email)?);
  }

  let email: Option<String> = email.map(validate_email).transpose()?;
  let unverified_email: Option<String> = unverified_email.map(validate_email).transpose()?;

  let user_id_bytes: [u8; 16] = user_id.into_bytes();
  let hashed_password = match password {
    Some(ref pw) => Some(hash_password(pw)?),
    None => None,
  };

  // NOTE: Empty string for username/email is used to unset ''.
  const UPDATE_QUERY: &str = formatcp!(
    "\
    UPDATE {USER_TABLE} SET \
      email = CASE :email \
        WHEN '' THEN NULL \
        ELSE COALESCE(:email, prev.email) \
      END, \
      unverified_email = CASE :unverified_email \
        WHEN '' THEN NULL \
        ELSE COALESCE(:unverified_email, prev.unverified_email) \
      END, \
      username = CASE :username \
        WHEN '' THEN NULL \
        ELSE COALESCE(:username, prev.username) \
      END, \
      password_hash = COALESCE(:password_hash, prev.password_hash) \
    FROM \
      (SELECT email, unverified_email, username, password_hash FROM {USER_TABLE} WHERE id = :id) AS prev \
    WHERE id = :id \
    "
  );

  return match state
    .user_conn()
    .execute(
      UPDATE_QUERY,
      named_params! {
          ":id": Value::Blob(user_id_bytes.to_vec()),
          ":email": if let Some(email) = email {
              Value::Text(email)
          } else {
              Value::Null
          },
          ":unverified_email": if let Some(unverified_email) = unverified_email {
              Value::Text(unverified_email)
          } else {
              Value::Null
          },
          ":username": if let Some(username) = username{
              if !username.is_empty() {
              Value::Text(validate_and_normalize_username(&username)?)
              } else {
              Value::Text(username)
              }
          } else {
              Value::Null
          },
          ":password_hash": hashed_password.map_or(Value::Null, Value::Text),
      },
    )
    .await?
  {
    0 => Ok((StatusCode::NOT_FOUND, "race?").into_response()),
    1 => Ok((StatusCode::OK, "updated").into_response()),
    _ => {
      unreachable!("user id must be unique");
    }
  };
}
