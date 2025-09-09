use axum::{
  extract::{FromRef, FromRequestParts, OptionalFromRequestParts},
  http::request::Parts,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthError;
use crate::auth::jwt::TokenClaims;
use crate::auth::tokens::extract_tokens_from_request_parts;
use crate::{app_state::AppState, util::b64_to_uuid};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct DbUser {
  pub id: [u8; 16],
  pub email: String,
  pub password_hash: String,
  pub verified: bool,
  pub admin: bool,

  pub created: i64,
  pub updated: i64,

  pub email_verification_code: Option<String>,
  pub email_verification_code_sent_at: Option<i64>,

  pub pending_email: Option<String>,

  pub password_reset_code: Option<String>,
  pub password_reset_code_sent_at: Option<i64>,

  pub authorization_code: Option<String>,
  pub authorization_code_sent_at: Option<i64>,
  pub pkce_code_challenge: Option<String>,

  // For external OAuth providers.
  //
  // NOTE: provider_id corresponds to proto::config::OAuthProviderId.
  pub provider_id: i64,
  pub provider_user_id: Option<String>,
  pub provider_avatar_url: Option<String>,
}

impl DbUser {
  pub(crate) fn uuid(&self) -> Uuid {
    let uuid = Uuid::from_bytes(self.id);
    return uuid;
  }

  #[cfg(test)]
  pub fn new_for_test(email: &str, password: &str) -> Self {
    let now = std::time::SystemTime::now();
    let timestamp = now.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();

    return Self {
      id: uuid::Uuid::new_v7(uuid::timestamp::Timestamp::from_unix(
        uuid::NoContext,
        timestamp,
        0,
      ))
      .into_bytes(),
      email: email.to_string(),
      password_hash: crate::auth::password::hash_password(password).unwrap(),
      verified: true,
      admin: false,
      created: timestamp as i64,
      updated: timestamp as i64,
      email_verification_code: None,
      email_verification_code_sent_at: None,
      pending_email: None,
      password_reset_code: None,
      password_reset_code_sent_at: None,
      authorization_code: None,
      authorization_code_sent_at: None,
      pkce_code_challenge: None,
      provider_id: 0,
      provider_user_id: None,
      provider_avatar_url: None,
    };
  }
}

/// Representing an authenticated and *valid* user, as opposed to DbUser, which is merely an entry
/// for any user including users that haven't been validated.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
  /// Url-safe Base64 encoded id of the current user.
  pub id: String,
  /// E-mail of the current user.
  pub email: String,
  /// Convenience UUID representation of [id] above.
  pub uuid: Uuid,

  /// The "expected" CSRF token as included in the auth token claims [User] was constructed from.
  pub csrf_token: String,
}

impl PartialEq for User {
  fn eq(&self, other: &User) -> bool {
    return self.id == other.id && self.email == other.email;
  }
}

impl User {
  /// Construct new verified [User] from [TokenClaims]. This is used when picking
  /// credentials/tokens from headers/cookies.
  pub(crate) fn from_token_claims(claims: TokenClaims) -> Result<Self, AuthError> {
    let uuid = b64_to_uuid(&claims.sub).map_err(|_err| AuthError::BadRequest("invalid user id"))?;

    return Ok(Self {
      id: claims.sub,
      email: claims.email,
      uuid,
      csrf_token: claims.csrf_token,
    });
  }

  #[cfg(test)]
  pub(crate) fn from_auth_token(state: &AppState, auth_token: &str) -> Option<Self> {
    Some(Self::from_token_claims(state.jwt().decode(auth_token).unwrap()).unwrap())
  }

  #[cfg(test)]
  pub(crate) fn from_unverified(user_id: Uuid, email: &str) -> Self {
    return Self {
      id: crate::util::uuid_to_b64(&user_id),
      email: email.to_string(),
      uuid: user_id,
      csrf_token: crate::rand::generate_random_string(20),
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
    let state = AppState::from_ref(state);
    let tokens = extract_tokens_from_request_parts(&state, parts).await?;

    let user = User::from_token_claims(tokens.auth_token_claims)?;

    tracing::Span::current().record("user_id", user.uuid.to_u128_le());

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
    let state = AppState::from_ref(state);

    if let Ok(tokens) = extract_tokens_from_request_parts(&state, parts).await {
      let user = User::from_token_claims(tokens.auth_token_claims)?;

      tracing::Span::current().record("user_id", user.uuid.to_u128_le());

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

  use crate::admin::user::create_user_for_test;
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
    state
      .jwt()
      .decode::<TokenClaims>(&tokens.auth_token)
      .unwrap();

    // Extract user from a request that only has a refresh token cookie but no auth token.
    // NOTE: non-cookie creds are not auto-refreshed.
    let request = Request::builder()
      .header(
        header::COOKIE,
        format!("{COOKIE_REFRESH_TOKEN}={}", tokens.refresh_token),
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
