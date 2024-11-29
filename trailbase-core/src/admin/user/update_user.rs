use axum::{
  extract::State,
  http::StatusCode,
  response::{IntoResponse, Response},
  Json,
};
use lazy_static::lazy_static;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::auth::password::hash_password;
use crate::constants::USER_TABLE;

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
  let conn = state.user_conn();
  let user_id_bytes = request.id.into_bytes();

  let hashed_password = match &request.password {
    Some(pw) => Some(hash_password(pw)?),
    None => None,
  };

  // TODO: Rather than using a transaction below we could build combined update queries:
  //   UPDATE <table> SET x = :x, y = :y WHERE id = :id.
  fn update_query(property: &str) -> String {
    format!("UPDATE '{USER_TABLE}' SET {property} = $1 WHERE id = $2")
  }

  lazy_static! {
    static ref UPDATE_EMAIL_QUERY: String = update_query("email");
    static ref UPDATE_PW_HASH_QUERY: String = update_query("password_hash");
    static ref UPDATE_VERIFIED_QUERY: String = update_query("verified");
  }

  let email = request.email.clone();
  let verified = request.verified;
  conn
    .call(move |conn| {
      let tx = conn.transaction()?;

      if let Some(email) = email {
        tx.execute(&UPDATE_EMAIL_QUERY, params![email, user_id_bytes])?;
      }
      if let Some(password_hash) = hashed_password {
        tx.execute(&UPDATE_PW_HASH_QUERY, params!(password_hash, user_id_bytes))?;
      }
      if let Some(verified) = verified {
        tx.execute(&UPDATE_VERIFIED_QUERY, params!(verified, user_id_bytes))?;
      }

      tx.commit()?;

      return Ok(());
    })
    .await?;

  return Ok((StatusCode::OK, format!("Updated user: {request:?}")).into_response());
}
