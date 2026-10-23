use axum::{
  extract::{FromRef, FromRequestParts, OptionalFromRequestParts},
  http::request::Parts,
};
use compact_str::CompactString;
use trailbase_sqlite::Row;
use uuid::Uuid;

use crate::auth::AuthError;
use crate::auth::jwt::AuthTokenClaims;
use crate::auth::tokens::extract_tokens_from_request_parts;
use crate::{app_state::AppState, util::b64_to_uuid};

#[derive(Debug, Default, Clone)]
pub struct DbUser {
  pub id: [u8; 16],
  pub email: Option<CompactString>,
  pub unverified_email: Option<CompactString>,
  pub username: Option<CompactString>,
  pub password_hash: Option<CompactString>,
  pub admin: bool,

  // TOTP secret for multi-factor.
  pub totp_secret: Option<CompactString>,

  pub created: i64,
  pub updated: i64,

  // For external OAuth providers.
  //
  // NOTE: provider_id corresponds to proto::config::OAuthProviderId.
  pub provider_id: i64,
  pub provider_user_id: Option<CompactString>,
  pub provider_avatar_url: Option<CompactString>,
}

impl DbUser {
  // NOTE: We rely on explicit column orders because we used ALTER COLUMN in PG leading to a
  // column order mismatch between SQLite and PG. Also, explicit is better :).
  pub const COLUMNS: &str = "id, email, unverified_email, username, password_hash, admin, totp_secret, created, updated, provider_id, provider_user_id, provider_avatar_url";

  pub fn uuid(&self) -> Uuid {
    return Uuid::from_bytes(self.id);
  }

  pub fn from_row(row: Row) -> Result<Self, AuthError> {
    #[inline]
    fn from_row_impl(mut row: Row) -> Result<DbUser, trailbase_sqlite::from_sql::FromSqlError> {
      // Sanity check.
      #[cfg(debug_assertions)]
      for (i, column) in DbUser::COLUMNS.split(",").enumerate() {
        assert_eq!(Some(column.trim()), row.column(i).map(|c| c.name.as_str()));
      }

      return Ok(DbUser {
        id: row.get(0)?,
        email: row.consume_value(1)?.try_into()?,
        unverified_email: row.consume_value(2)?.try_into()?,
        username: row.consume_value(3)?.try_into()?,
        password_hash: row.consume_value(4)?.try_into()?,
        admin: row.get(5)?,
        totp_secret: row.consume_value(6)?.try_into()?,
        created: row.get(7)?,
        updated: row.get(8)?,
        provider_id: row.get(9)?,
        provider_user_id: row.consume_value(10)?.try_into()?,
        provider_avatar_url: row.consume_value(11)?.try_into()?,
      });
    }

    // Should never fail. This means there's a schema mismatch.
    return from_row_impl(row).map_err(|err| AuthError::Internal(err.into()));
  }

  #[cfg(test)]
  pub fn new_for_test(email: &str, password: &str) -> Self {
    use compact_str::ToCompactString;

    let now = std::time::SystemTime::now();
    let timestamp = now.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();

    return Self {
      id: uuid::Uuid::new_v4().into_bytes(),
      email: Some(email.to_compact_string()),
      unverified_email: None,
      username: None,
      password_hash: Some(
        crate::auth::password::hash_password_impl(password)
          .unwrap()
          .to_compact_string(),
      ),
      admin: false,
      totp_secret: None,
      created: timestamp as i64,
      updated: timestamp as i64,
      provider_id: 0,
      provider_user_id: None,
      provider_avatar_url: None,
    };
  }
}

/// Representing an authenticated and *valid* user, as opposed to [`DbUser`], which is merely an
/// entry for any user including users that haven't been validated.
#[derive(Clone, Debug)]
pub struct DbUserAndTenant {
  pub user: DbUser,
  pub tenant: Option<[u8; 16]>,
}

impl DbUserAndTenant {
  // pub const COLUMNS: &str = formatcp!("{}, tenant", DbUser::COLUMNS);

  pub fn from_row(row: Row) -> Result<Self, AuthError> {
    // Sanity check.
    debug_assert_eq!(Some("tenant"), row.column(12).map(|c| c.name.as_str()));

    return Ok(Self {
      tenant: row.get(12).map_err(|err| AuthError::Internal(err.into()))?,
      user: DbUser::from_row(row)?,
    });
  }
}

/// Representing an authenticated and *valid* user, as opposed to DbUser, which is merely an entry
/// for any user including users that haven't been validated.
#[derive(Clone, Debug)]
pub struct User {
  /// UUID representation of the current user.
  pub id: Uuid,
  /// UUID representation of the currently active tenant association.
  pub tenant: Option<Uuid>,

  /// E-mail of the current user.
  pub email: Option<CompactString>,
  /// Username of the current user.
  pub username: Option<CompactString>,

  /// The "expected" CSRF token as included in the auth token claims [`User`] was constructed from.
  pub csrf_token: CompactString,
}

impl PartialEq for User {
  fn eq(&self, other: &User) -> bool {
    return self.id == other.id && self.email == other.email;
  }
}

impl User {
  /// Construct new verified [User] from [AuthTokenClaims]. This is used when picking
  /// credentials/tokens from headers/cookies.
  pub(crate) fn from_token_claims(claims: AuthTokenClaims) -> Result<Self, AuthError> {
    let id = b64_to_uuid(&claims.sub).map_err(|_err| AuthError::BadRequest("invalid user id"))?;
    let tenant = claims
      .tenant
      .as_deref()
      .map(b64_to_uuid)
      .transpose()
      .map_err(|_err| AuthError::BadRequest("invalid tenant id"))?;

    return Ok(Self {
      id,
      tenant,
      email: claims.email,
      username: claims.username,
      csrf_token: claims.csrf_token,
    });
  }

  #[cfg(test)]
  pub(crate) fn from_auth_token(state: &AppState, auth_token: &str) -> Option<Self> {
    Some(
      Self::from_token_claims(AuthTokenClaims::from_auth_token(state.jwt(), auth_token).unwrap())
        .unwrap(),
    )
  }

  #[cfg(test)]
  pub(crate) fn from_unverified(
    user_id: Uuid,
    email: Option<&str>,
    username: Option<&str>,
  ) -> Self {
    use compact_str::ToCompactString;

    return Self {
      id: user_id,
      tenant: None,
      email: email.map(|s| s.to_compact_string()),
      username: username.map(|s| s.to_compact_string()),
      csrf_token: crate::rand::random_alphanumeric(24).to_compact_string(),
    };
  }
}

impl<S> FromRequestParts<S> for User
where
  AppState: FromRef<S>,
  S: Send + Sync,
{
  type Rejection = AuthError;

  async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
    let user = User::from_token_claims(
      extract_tokens_from_request_parts(&AppState::from_ref(state), parts)
        .await?
        .auth_token_claims,
    )?;

    tracing::Span::current().record("user_id", user.id.as_u128());

    return Ok(user);
  }
}

impl<S> OptionalFromRequestParts<S> for User
where
  AppState: FromRef<S>,
  S: Send + Sync,
{
  type Rejection = AuthError;

  async fn from_request_parts(
    parts: &mut Parts,
    state: &S,
  ) -> Result<Option<Self>, Self::Rejection> {
    if let Ok(tokens) = extract_tokens_from_request_parts(&AppState::from_ref(state), parts).await {
      let user = User::from_token_claims(tokens.auth_token_claims)?;

      tracing::Span::current().record("user_id", user.id.as_u128());

      return Ok(Some(user));
    }
    return Ok(None);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::body::Body;
  use axum::http::{Request, header};

  use crate::admin::user::create_user::create_user_for_test;
  use crate::app_state::test_state;
  use crate::auth::util::login_with_password;
  use crate::constants::COOKIE_REFRESH_TOKEN;

  #[tokio::test]
  async fn test_token_refresh() {
    let state = test_state(None).await.unwrap();

    let email = "name@bar.com".to_string();
    let password = "secret123".to_string();

    let user_id = create_user_for_test(&state, &email, &password)
      .await
      .unwrap();

    let tokens = login_with_password(&state, &email, &password)
      .await
      .unwrap();
    assert_eq!(tokens.id, user_id);
    AuthTokenClaims::from_auth_token(state.jwt(), &tokens.auth_token).unwrap();

    // Extract user from a request that only has a refresh token cookie but no auth token.
    // NOTE: non-cookie creds are not auto-refreshed.
    let request = Request::builder()
      .header(
        header::COOKIE,
        format!("{COOKIE_REFRESH_TOKEN}={}", tokens.refresh_token.unwrap()),
      )
      .body(Body::empty())
      .unwrap();

    let (mut parts, _body) = request.into_parts();

    // Emulate the tower_cookies::CookieManagerLayer.
    let cookies = tower_cookies::Cookies::default();
    cookies.add(
      tower_cookies::Cookie::parse(
        parts
          .headers
          .get(header::COOKIE)
          .unwrap()
          .to_str()
          .unwrap()
          .to_string(),
      )
      .unwrap(),
    );
    parts.extensions.insert(cookies);

    <User as FromRequestParts<AppState>>::from_request_parts(&mut parts, &state)
      .await
      .unwrap();
  }
}
