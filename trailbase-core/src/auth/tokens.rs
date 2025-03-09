use axum::{
  extract::{FromRef, FromRequestParts, OptionalFromRequestParts},
  http::{header, request::Parts},
};
use chrono::Duration;
use lazy_static::lazy_static;
use tower_cookies::Cookies;
use trailbase_sqlite::params;

use crate::app_state::AppState;
use crate::auth::jwt::TokenClaims;
use crate::auth::user::DbUser;
use crate::auth::util::new_cookie;
use crate::auth::AuthError;
use crate::constants::{
  COOKIE_AUTH_TOKEN, COOKIE_REFRESH_TOKEN, HEADER_REFRESH_TOKEN, REFRESH_TOKEN_LENGTH,
  SESSION_TABLE, USER_TABLE,
};
use crate::rand::generate_random_string;
use crate::util::{get_header, get_header_owned};

#[derive(Clone)]
pub(crate) struct Tokens {
  pub auth_token_claims: TokenClaims,
  pub refresh_token: Option<String>,
}

impl<S> FromRequestParts<S> for Tokens
where
  AppState: FromRef<S>,
  S: Send + Sync,
{
  type Rejection = AuthError;

  async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
    let state = AppState::from_ref(state);
    return extract_tokens_from_request_parts(&state, parts).await;
  }
}

impl<S> OptionalFromRequestParts<S> for Tokens
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
    return Ok(extract_tokens_from_request_parts(&state, parts).await.ok());
  }
}

#[inline]
pub(crate) async fn extract_tokens_from_request_parts(
  state: &AppState,
  parts: &Parts,
) -> Result<Tokens, AuthError> {
  if let Ok(tokens) = extract_tokens_from_headers(state, &parts.headers) {
    return Ok(tokens);
  }

  let Some(cookies) = parts.extensions.get::<Cookies>() else {
    // This like means the tower_cookies::CookieManagerLayer isn't installed.
    return Err(AuthError::Internal("cookie error".into()));
  };

  return extract_tokens_from_cookies_and_maybe_refresh(state, cookies).await;
}

#[inline]
pub(crate) fn extract_token_claims_from_headers(
  state: &AppState,
  headers: &header::HeaderMap,
) -> Result<TokenClaims, AuthError> {
  let Some(auth_token_str) =
    get_header(headers, header::AUTHORIZATION).and_then(|v| v.strip_prefix("Bearer "))
  else {
    return Err(AuthError::Unauthorized);
  };

  return state
    .jwt()
    .decode(auth_token_str)
    .map_err(|_err| AuthError::Unauthorized);
}

#[inline]
fn extract_tokens_from_headers(
  state: &AppState,
  headers: &header::HeaderMap,
) -> Result<Tokens, AuthError> {
  return Ok(Tokens {
    auth_token_claims: extract_token_claims_from_headers(state, headers)?,
    refresh_token: get_header_owned(headers, HEADER_REFRESH_TOKEN),
  });
}

async fn extract_tokens_from_cookies_and_maybe_refresh(
  state: &AppState,
  cookies: &Cookies,
) -> Result<Tokens, AuthError> {
  let auth_token = cookies.get(COOKIE_AUTH_TOKEN);

  let refresh_token = cookies
    .get(COOKIE_REFRESH_TOKEN)
    .map(|cookie| cookie.value().to_string());

  if let Some(ref auth_token) = auth_token {
    if let Ok(claims) = state.jwt().decode(auth_token.value()) {
      return Ok(Tokens {
        auth_token_claims: claims,
        refresh_token,
      });
    }
  }

  if let Some(refresh_token) = refresh_token {
    // Try to auto-refresh in the cookie-case only (otherwise we don't have a back channel. If were
    // to rely on a client lib to pick it from the response headers we might as well give the
    // client the responsibility to explicitly refresh).
    let (auth_token_ttl, refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());
    let claims = reauth_with_refresh_token(
      state,
      refresh_token.clone(),
      refresh_token_ttl,
      auth_token_ttl,
    )
    .await?;

    let new_token = state
      .jwt()
      .encode(&claims)
      .map_err(|err| AuthError::Internal(err.into()))?;

    cookies.add(new_cookie(
      COOKIE_AUTH_TOKEN,
      new_token,
      auth_token_ttl,
      state.dev_mode(),
    ));

    return Ok(Tokens {
      auth_token_claims: claims,
      refresh_token: Some(refresh_token),
    });
  }

  return Err(AuthError::Unauthorized);
}

/// Only difference to Tokens above, refresh token presence is guaranteed.
pub struct FreshTokens {
  pub auth_token_claims: TokenClaims,
  pub refresh_token: String,
}

pub(crate) async fn mint_new_tokens(
  state: &AppState,
  verified: bool,
  user_id: uuid::Uuid,
  user_email: String,
  expires_in: Duration,
) -> Result<FreshTokens, AuthError> {
  assert!(verified);
  if !verified {
    return Err(AuthError::Internal(
      "Cannot mint tokens for unverified user".into(),
    ));
  }

  let claims = TokenClaims::new(verified, user_id, user_email, expires_in);

  // Unlike JWT auth tokens, refresh tokens are opaque.
  let refresh_token = generate_random_string(REFRESH_TOKEN_LENGTH);
  lazy_static! {
    static ref QUERY: String =
      format!("INSERT INTO '{SESSION_TABLE}' (user, refresh_token) VALUES ($1, $2)");
  }

  state
    .user_conn()
    .execute(
      &QUERY,
      params!(user_id.into_bytes().to_vec(), refresh_token.clone(),),
    )
    .await?;

  return Ok(FreshTokens {
    auth_token_claims: claims,
    refresh_token,
  });
}

pub(crate) async fn reauth_with_refresh_token(
  state: &AppState,
  refresh_token: String,
  refresh_token_ttl: Duration,
  auth_token_ttl: Duration,
) -> Result<TokenClaims, AuthError> {
  lazy_static! {
    static ref QUERY: String = format!(
      r#"
        SELECT user.*
        FROM
          {SESSION_TABLE} AS s
          INNER JOIN {USER_TABLE} AS user ON s.user = user.id
        WHERE
          s.refresh_token = $1 AND s.updated > (UNIXEPOCH() - $2) AND user.verified
      "#
    );
  }

  let Some(db_user) = state
    .user_conn()
    .query_value::<DbUser>(
      &QUERY,
      params!(refresh_token, refresh_token_ttl.num_seconds()),
    )
    .await?
  else {
    // Row not found case, typically expected in one of 4 cases:
    //  1. Above where clause doesn't match, e.g. refresh token expired.
    //  2. Token was actively deleted and thus revoked.
    //  3. User explicitly logged out, which will delete **all** sessions for that user.
    //  4. Database was overwritten, e.g. by tests or periodic reset for the demo.
    #[cfg(debug_assertions)]
    log::debug!("Refresh token not found");

    return Err(AuthError::Unauthorized);
  };

  assert!(
    db_user.verified,
    "unverified user, should have been caught by above query"
  );

  return Ok(TokenClaims::new(
    db_user.verified,
    db_user.uuid(),
    db_user.email,
    auth_token_ttl,
  ));
}
