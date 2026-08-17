use base64::prelude::*;
use const_format::formatcp;
use serde::{Deserialize, Serialize};
use trailbase_sqlite::traits::{SyncConnection, SyncTransaction};
use trailbase_sqlite::{Connection, params};
use uuid::Uuid;
use validator::ValidateEmail;

use crate::DataDir;
use crate::auth::AuthError;
use crate::auth::password::hash_password;
use crate::auth::tokens::{FreshTokens, mint_new_tokens};
use crate::auth::user::DbUser;
use crate::auth::util::{
  get_user_by_email, get_user_by_id, get_user_by_username, validate_and_normalize_email_address,
  validate_and_normalize_username,
};
use crate::constants::USER_TABLE;

pub enum UserReference {
  Email(String),
  Username(String),
  Id(uuid::Uuid),
}

impl UserReference {
  pub fn parse(user: impl AsRef<str>) -> Result<Self, String> {
    let user = user.as_ref().trim().to_string();
    if user.contains("@") {
      if !user.validate_email() {
        return Err(format!("invalid email address: {user}"));
      }
      return Ok(Self::Email(user));
    }

    // TODO: We could do more validation, e.g. username characters, UUID version, etc.
    return match user.len() {
      36 if let Ok(uuid) = Uuid::parse_str(&user) => Ok(Self::Id(uuid)),
      24 if let Ok(base64) = BASE64_URL_SAFE.decode(&user) => Ok(Self::Id(
        Uuid::from_slice(&base64).map_err(|err| err.to_string())?,
      )),
      _ => Ok(Self::Username(user)),
    };
  }

  async fn lookup_user(&self, user_conn: &Connection) -> Result<DbUser, AuthError> {
    return match self {
      Self::Email(email) => get_user_by_email(user_conn, email).await,
      Self::Username(username) => get_user_by_username(user_conn, username).await,
      Self::Id(uuid) => get_user_by_id(user_conn, uuid).await,
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
    formatcp!(r#"UPDATE "{USER_TABLE}" SET password_hash = $1 WHERE id = $2 RETURNING id"#);

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
    formatcp!(r#"UPDATE "{USER_TABLE}" SET email = $1 WHERE id = $2 RETURNING id"#);

  return user_conn
    .write_query_value(UPDATE_EMAIL_QUERY, params!(normalized_email, db_user.id))
    .await?
    .ok_or(AuthError::NotFound);
}

pub async fn change_username(
  user_conn: &trailbase_sqlite::Connection,
  user: UserReference,
  new_username: &str,
) -> Result<Uuid, AuthError> {
  let normalized_username = validate_and_normalize_username(new_username)?;
  let db_user = user.lookup_user(user_conn).await?;

  const UPDATE_USERNAME_QUERY: &str =
    formatcp!(r#"UPDATE "{USER_TABLE}" SET username = $1 WHERE id = $2 RETURNING id"#);

  return user_conn
    .write_query_value(
      UPDATE_USERNAME_QUERY,
      params!(normalized_username, db_user.id),
    )
    .await?
    .ok_or(AuthError::NotFound);
}

pub async fn add_user(
  user_conn: &trailbase_sqlite::Connection,
  email: &str,
  password: &str,
) -> Result<Uuid, AuthError> {
  const ADD_USER_QUERY: &str = formatcp!(
    r#"INSERT INTO "{USER_TABLE}" (email, password_hash, verified) VALUES ($1, $2, $3) RETURNING *"#
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

  const DELETE_QUERY: &str = formatcp!(r#"DELETE FROM "{USER_TABLE}" WHERE id = $1"#);

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
    formatcp!(r#"UPDATE "{USER_TABLE}" SET verified = $1 WHERE id = $2 RETURNING id"#);

  return user_conn
    .write_query_value(SET_VERIFIED_QUERY, params!(verified, db_user.id))
    .await?
    .ok_or(AuthError::NotFound);
}

pub async fn invalidate_sessions(
  user_conn: &trailbase_sqlite::Connection,
  session_conn: &trailbase_sqlite::Connection,
  user: UserReference,
) -> Result<(), AuthError> {
  let db_user = user.lookup_user(user_conn).await?;

  crate::auth::util::delete_all_sessions_for_user(session_conn, Uuid::from_bytes(db_user.id))
    .await?;

  return Ok(());
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthTokens {
  pub auth_token: String,
  pub refresh_token: String,
}

pub async fn mint_auth_tokens(
  data_dir: &DataDir,
  user_conn: &trailbase_sqlite::Connection,
  session_conn: &trailbase_sqlite::Connection,
  user: UserReference,
) -> Result<AuthTokens, AuthError> {
  let jwt = crate::api::JwtHelper::init_from_path(data_dir)
    .await
    .map_err(|err| AuthError::FailedDependency(err.into()))?;
  let db_user = user.lookup_user(user_conn).await?;

  // NOTE: we just discard the refresh token.
  let auth_token_ttl = chrono::Duration::hours(12);
  let refresh_token_ttl = chrono::Duration::hours(12);
  let FreshTokens {
    auth_token_claims,
    refresh_token,
  } = mint_new_tokens(session_conn, &db_user, &auth_token_ttl, &refresh_token_ttl).await?;

  let auth_token = jwt
    .encode(&auth_token_claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(AuthTokens {
    auth_token,
    refresh_token,
  });
}

pub async fn promote_user_to_admin(
  user_conn: &trailbase_sqlite::Connection,
  user: UserReference,
) -> Result<Uuid, AuthError> {
  let db_user = user.lookup_user(user_conn).await?;

  const PROMOTE_ADMIN_QUERY: &str =
    formatcp!(r#"UPDATE "{USER_TABLE}" SET admin = TRUE WHERE id = $1 RETURNING id"#);

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
    formatcp!(r#"UPDATE "{USER_TABLE}" SET admin = FALSE WHERE id = $1 RETURNING id"#);

  return user_conn
    .write_query_value(DEMOTE_ADMIN_QUERY, params!(db_user.id))
    .await?
    .ok_or(AuthError::NotFound);
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportUser {
  pub email: String,
  pub password_hash: String,
  pub verified: bool,
}

pub async fn import_users(
  user_conn: &trailbase_sqlite::Connection,
  users: Vec<ImportUser>,
) -> Result<(), AuthError> {
  // First validate the users.
  for user in &users {
    if !trailbase_extension::password::valid_hash(&user.password_hash) {
      return Err(AuthError::BadRequest("Invalid Hash"));
    }

    let _ = validate_and_normalize_email_address(&user.email)?;
  }

  user_conn
    .transaction(|mut tx| -> Result<(), trailbase_sqlite::Error> {
      const IMPORT_USER_QUERY: &str = formatcp!(
        r#"INSERT INTO "{USER_TABLE}" (email, password_hash, verified) VALUES ($1, $2, $3)"#
      );

      for user in users {
        let email = user.email;
        tx.execute(
          IMPORT_USER_QUERY,
          params!(email.clone(), user.password_hash, user.verified),
        )
        .map_err(|err| {
          trailbase_sqlite::Error::Other(format!("Failed to insert '{email}':{err}").into())
        })?;
      }

      tx.commit()
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

      return Ok(());
    })
    .await
    .map_err(|err| AuthError::FailedDependency(err.into()))?;

  return Ok(());
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_user_reference() {
    let UserReference::Email(email) = UserReference::parse("  user@test.org ").unwrap() else {
      panic!("expected email");
    };
    assert_eq!(email, "user@test.org");

    let UserReference::Username(username) = UserReference::parse("  foo ").unwrap() else {
      panic!("expected username");
    };
    assert_eq!(username, "foo");

    let uuid = uuid::Uuid::new_v4();
    let UserReference::Id(_) = UserReference::parse(uuid.to_string()).unwrap() else {
      panic!("expected uuid");
    };

    let UserReference::Id(_) =
      UserReference::parse(BASE64_URL_SAFE.encode(uuid.into_bytes())).unwrap()
    else {
      panic!("expected uuid");
    };
  }
}
