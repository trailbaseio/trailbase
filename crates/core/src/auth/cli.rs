use base64::prelude::*;
use const_format::formatcp;
use trailbase_sqlite::{Connection, params};
use uuid::Uuid;

use crate::DataDir;
use crate::auth::AuthError;
use crate::auth::password::hash_password;
use crate::auth::tokens::mint_new_tokens;
use crate::auth::user::DbUser;
use crate::auth::util::{get_user_by_email, get_user_by_id, validate_and_normalize_email_address};
use crate::constants::USER_TABLE;

pub enum UserReference {
  Email(String),
  Id(String),
}

impl UserReference {
  async fn lookup_user(&self, user_conn: &Connection) -> Result<DbUser, AuthError> {
    return match self {
      Self::Email(email) => get_user_by_email(user_conn, email).await,
      Self::Id(id) => {
        let decoded_id = Uuid::parse_str(id).or_else(|_| {
          let bytes = BASE64_URL_SAFE.decode(id).map_err(|err| {
            AuthError::FailedDependency(format!("Failed to parse Base64: {err}").into())
          })?;
          return Uuid::from_slice(&bytes).map_err(|err| {
            AuthError::FailedDependency(format!("Failed to parse UUID from slice: {err}").into())
          });
        })?;
        get_user_by_id(user_conn, &decoded_id).await
      }
    };
  }
}

pub async fn change_password(
  user_conn: &trailbase_sqlite::Connection,
  user: UserReference,
  password: &str,
) -> Result<Uuid, AuthError> {
  let db_user = user.lookup_user(user_conn).await?;

  let hashed_password = hash_password(password)?;

  const UPDATE_PASSWORD_QUERY: &str =
    formatcp!("UPDATE '{USER_TABLE}' SET password_hash = $1 WHERE id = $2 RETURNING id");

  return user_conn
    .write_query_value(UPDATE_PASSWORD_QUERY, params!(hashed_password, db_user.id))
    .await?
    .ok_or(AuthError::NotFound);
}

pub async fn change_email(
  user_conn: &trailbase_sqlite::Connection,
  user: UserReference,
  new_email: &str,
) -> Result<Uuid, AuthError> {
  let normalized_email = validate_and_normalize_email_address(new_email)?;
  let db_user = user.lookup_user(user_conn).await?;

  const UPDATE_EMAIL_QUERY: &str =
    formatcp!("UPDATE '{USER_TABLE}' SET email = $1 WHERE id = $2 RETURNING id");

  return user_conn
    .write_query_value(UPDATE_EMAIL_QUERY, params!(normalized_email, db_user.id))
    .await?
    .ok_or(AuthError::NotFound);
}

pub async fn add_user(
  user_conn: &trailbase_sqlite::Connection,
  email: &str,
  password: &str,
) -> Result<Uuid, AuthError> {
  const ADD_USER_QUERY: &str = formatcp!(
    "INSERT INTO '{USER_TABLE}' (email, password_hash, verified) VALUES (?1, ?2, ?3) RETURNING *"
  );

  let normalized_email = validate_and_normalize_email_address(email)?;
  if password.is_empty() {
    return Err(AuthError::BadRequest("Password must not be empty"));
  }
  let hashed_password = hash_password(password)?;

  let user: DbUser = user_conn
    .write_query_value(
      ADD_USER_QUERY,
      params!(normalized_email, hashed_password, true),
    )
    .await?
    .ok_or(AuthError::NotFound)?;

  return Ok(user.uuid());
}

pub async fn delete_user(
  user_conn: &trailbase_sqlite::Connection,
  user: UserReference,
) -> Result<(), AuthError> {
  let db_user = user.lookup_user(user_conn).await?;

  const DELETE_QUERY: &str = formatcp!("DELETE FROM '{USER_TABLE}' WHERE id = $1");

  let rows_affected = user_conn.execute(DELETE_QUERY, params!(db_user.id)).await?;
  if rows_affected > 0 {
    return Ok(());
  }

  return Err(AuthError::NotFound);
}

pub async fn set_verified(
  user_conn: &trailbase_sqlite::Connection,
  user: UserReference,
  verified: bool,
) -> Result<Uuid, AuthError> {
  let db_user = user.lookup_user(user_conn).await?;

  const SET_VERIFIED_QUERY: &str =
    formatcp!("UPDATE '{USER_TABLE}' SET verified = $1 WHERE id = $2 RETURNING id");

  return user_conn
    .write_query_value(SET_VERIFIED_QUERY, params!(verified, db_user.id))
    .await?
    .ok_or(AuthError::NotFound);
}

pub async fn invalidate_sessions(
  user_conn: &trailbase_sqlite::Connection,
  user: UserReference,
) -> Result<(), AuthError> {
  let db_user = user.lookup_user(user_conn).await?;

  crate::auth::util::delete_all_sessions_for_user(user_conn, Uuid::from_bytes(db_user.id)).await?;

  return Ok(());
}

pub async fn mint_auth_token(
  data_dir: &DataDir,
  user_conn: &trailbase_sqlite::Connection,
  user: UserReference,
) -> Result<String, AuthError> {
  let jwt = crate::api::JwtHelper::init_from_path(data_dir)
    .await
    .map_err(|err| AuthError::FailedDependency(err.into()))?;
  let db_user = user.lookup_user(user_conn).await?;

  let tokens = mint_new_tokens(user_conn, &db_user, chrono::Duration::hours(12)).await?;

  let auth_token = jwt
    .encode(&tokens.auth_token_claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(auth_token);
}

pub async fn promote_user_to_admin(
  user_conn: &trailbase_sqlite::Connection,
  user: UserReference,
) -> Result<Uuid, AuthError> {
  let db_user = user.lookup_user(user_conn).await?;

  const PROMOTE_ADMIN_QUERY: &str =
    formatcp!("UPDATE '{USER_TABLE}' SET admin = TRUE WHERE id = $1 RETURNING id");

  return user_conn
    .write_query_value(PROMOTE_ADMIN_QUERY, params!(db_user.id))
    .await?
    .ok_or(AuthError::NotFound);
}

pub async fn demote_admin_to_user(
  user_conn: &trailbase_sqlite::Connection,
  user: UserReference,
) -> Result<Uuid, AuthError> {
  let db_user = user.lookup_user(user_conn).await?;

  const DEMOTE_ADMIN_QUERY: &str =
    formatcp!("UPDATE '{USER_TABLE}' SET admin = FALSE WHERE id = $1 RETURNING id");

  return user_conn
    .write_query_value(DEMOTE_ADMIN_QUERY, params!(db_user.id))
    .await?
    .ok_or(AuthError::NotFound);
}
