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
  password: Option<String>,
  verified: Option<bool>,
}

pub async fn update_user_handler(
  State(state): State<AppState>,
  Json(request): Json<UpdateUserRequest>,
) -> Result<Response, Error> {
  let UpdateUserRequest {
    id: user_id,
    email,
    password,
    verified,
  } = request;

  if is_admin(&state, &user_id).await {
    return Err(Error::Precondition(
      "Admins can only be updated using the CLI to prevent abuse".into(),
    ));
  }

  let user_id_bytes: [u8; 16] = user_id.into_bytes();
  let hashed_password = match password {
    Some(ref pw) => Some(hash_password(pw)?),
    None => None,
  };

  const UPDATE_QUERY: &str = formatcp!(
    "
    UPDATE {USER_TABLE} SET
      email = COALESCE(:email, prev.email),
      password_hash = COALESCE(:password_hash, prev.password_hash),
      verified = COALESCE(:verified, prev.verified)
    FROM
      (SELECT email, password_hash, verified FROM {USER_TABLE} WHERE id = :id) AS prev
    WHERE id = :id
    "
  );

  return match state
    .user_conn()
    .execute(
      UPDATE_QUERY,
      named_params! {
          ":id": Value::Blob(user_id_bytes.to_vec()),
          ":email": email.map_or(Value::Null, Value::Text),
          ":password_hash": hashed_password.map_or(Value::Null, Value::Text),
          ":verified": verified.map_or(Value::Null, |v| Value::Integer(if v {1} else {0})),
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
