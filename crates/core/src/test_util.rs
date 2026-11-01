use axum::extract::{Json, State};
use axum::response::Response;
use std::string::ToString;
use trailbase_sqlite::{Connection, ConnectionType};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::util::{user_by_email, user_by_username};
use crate::auth::{AuthError, DbUser};

pub type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Debug)]
pub enum UserIdentifier {
  Email(String),
  Username(String),
}

#[derive(Debug)]
pub struct TestUser {
  pub id: Uuid,
  pub user: UserIdentifier,
  pub password: String,
}

pub async fn create_user_for_test(
  state: &AppState,
  user: UserIdentifier,
  password: impl ToString,
) -> TestUser {
  use crate::admin::user::create_user::{CreateUserRequest, create_user_handler};

  let response = create_user_handler(
    State(state.clone()),
    Json(match &user {
      UserIdentifier::Username(username) => CreateUserRequest {
        email: None,
        username: Some(username.clone()),
        password: password.to_string(),
        verified: true,
        admin: false,
      },
      UserIdentifier::Email(email) => CreateUserRequest {
        email: Some(email.clone()),
        username: None,
        password: password.to_string(),
        verified: true,
        admin: false,
      },
    }),
  )
  .await
  .expect("test-only");

  return TestUser {
    id: response.id,
    user,
    password: password.to_string(),
  };
}

#[derive(Debug)]
pub struct LoginResult {
  pub id: uuid::Uuid,
  pub auth_token: String,
  pub refresh_token: Option<String>,
  pub csrf_token: String,
}

pub async fn login_user_for_test(
  state: &AppState,
  user: UserIdentifier,
  password: &str,
) -> Result<LoginResult, AuthError> {
  let db_user: DbUser = match &user {
    UserIdentifier::Username(username) => user_by_username(state, username).await,
    UserIdentifier::Email(email) => user_by_email(state, email).await,
  }
  .map_err(|_| AuthError::NotFound)?;

  // Validates password and rate limits attempts.
  crate::auth::password::check_user_password(&db_user, password.to_string()).await?;

  let crate::auth::tokens::FreshTokens {
    auth_token_claims: (auth_token_claims, _ttl),
    refresh_token,
  } = crate::auth::tokens::mint_new_tokens(state, &db_user, None).await?;

  let auth_token = state
    .jwt()
    .encode(&auth_token_claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(LoginResult {
    id: db_user.uuid(),
    auth_token,
    refresh_token: refresh_token.map(|(token, _ttl)| token),
    csrf_token: auth_token_claims.csrf_token.into_string(),
  });
}

pub struct TestUserAndLogin {
  pub user: TestUser,
  pub login: LoginResult,
}

pub async fn create_and_login_user_for_test(
  state: &AppState,
  user: UserIdentifier,
  password: impl ToString,
) -> Result<TestUserAndLogin, AuthError> {
  let user = create_user_for_test(state, user, password).await;
  let login = login_user_for_test(state, user.user.clone(), &user.password).await?;

  assert_eq!(login.id, user.id);

  return Ok(TestUserAndLogin { login, user });
}

pub fn strict(conn: &Connection) -> &'static str {
  return match conn.connection_type() {
    ConnectionType::Pg => "",
    ConnectionType::Sqlite => "STRICT",
  };
}

pub fn uuid_column(conn: &Connection) -> &'static str {
  return match conn.connection_type() {
    ConnectionType::Pg => "UUID",
    ConnectionType::Sqlite => "BLOB",
  };
}

pub fn blob_column(conn: &Connection) -> &'static str {
  return match conn.connection_type() {
    ConnectionType::Pg => "BYTEA",
    ConnectionType::Sqlite => "BLOB",
  };
}

pub fn json_column(conn: &Connection) -> &'static str {
  return match conn.connection_type() {
    ConnectionType::Pg => "JSONB",
    ConnectionType::Sqlite => "TEXT",
  };
}

pub fn serial_column(conn: &Connection) -> &'static str {
  return match conn.connection_type() {
    ConnectionType::Pg => "BIGSERIAL",
    ConnectionType::Sqlite => "INTEGER",
  };
}

pub async fn unpack_json_response<T: for<'a> serde::Deserialize<'a>>(
  response: Response,
) -> Result<T, AnyError> {
  let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
  return Ok(serde_json::from_slice::<T>(&bytes)?);
}
