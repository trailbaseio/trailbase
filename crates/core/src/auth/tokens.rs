use axum::{
  extract::{FromRef, FromRequestParts, OptionalFromRequestParts},
  http::{header, request::Parts},
};
use chrono::Duration;
use const_format::formatcp;
use tower_cookies::Cookies;
use trailbase_sqlite::{SyncConnectionTrait, params};

use crate::auth::AuthError;
use crate::auth::jwt::AuthTokenClaims;
use crate::auth::user::DbUser;
use crate::auth::util::{SameSite, new_cookie};
use crate::constants::{
  COOKIE_AUTH_TOKEN, COOKIE_REFRESH_TOKEN, HEADER_REFRESH_TOKEN, REFRESH_TOKEN_LENGTH,
  SESSION_TABLE, USER_TABLE,
};
use crate::rand::random_alphanumeric;
use crate::util::get_header;
use crate::{app_state::AppState, auth::user::DbUserAndTenant};

#[derive(Clone, Debug)]
pub(crate) struct Tokens {
  pub auth_token_claims: AuthTokenClaims,
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
  // Headers take priority over cookies.
  //
  // NOTE: We don't "auto-refresh" stale auth tokens in the header case, since we don't have the
  // means to propagate the new token back (unlike for cookies). The responsibility sits with the
  // client to refresh tokens in time.
  if let Some(tokens) = extract_tokens_from_headers(&parts.headers) {
    let claims = AuthTokenClaims::from_auth_token(state.jwt(), tokens.auth_token)
      .map_err(|_| AuthError::Unauthorized)?;

    return Ok(Tokens {
      auth_token_claims: claims,
      refresh_token: tokens.refresh_token.map(|r| r.to_owned()),
    });
  }

  // Fall back to cookies.
  let Some(cookies) = parts.extensions.get::<Cookies>() else {
    debug_assert!(false);
    // This like means the tower_cookies::CookieManagerLayer isn't installed.
    return Err(AuthError::Internal("cookie error".into()));
  };

  if let Some(tokens) = extract_tokens_from_cookies(cookies) {
    // If an auth-token is present, first try that before falling back to "auto-refresh".
    if let Some(auth_token) = &tokens.auth_token
      && let Ok(claims) = AuthTokenClaims::from_auth_token(state.jwt(), auth_token)
        .map_err(|_| AuthError::Unauthorized)
    {
      return Ok(Tokens {
        auth_token_claims: claims,
        refresh_token: tokens.refresh_token.map(|r| r.to_owned()),
      });
    }

    // Unlike in the header case, we can "auto-refresh" a stale auth token, since we have a
    // back-channel to propagate the new token. Also for "dumb" clients, we need to do this, since
    // they may not be able to take on the refresh responsibility.
    if let Some(refresh_token) = tokens.refresh_token {
      let (claims, ttl) = reauth_with_refresh_token(state, refresh_token.clone()).await?;

      let new_auth_token = state.jwt().encode(&claims).map_err(|err| {
        debug_assert!(false);
        // Freshly minted token should decode just fine. Otherwise something is wrong.
        return AuthError::Internal(err.into());
      })?;

      cookies.add(new_cookie(
        state,
        COOKIE_AUTH_TOKEN,
        new_auth_token,
        ttl,
        SameSite::Strict,
      ));

      return Ok(Tokens {
        auth_token_claims: claims,
        refresh_token: Some(refresh_token),
      });
    }
  }

  return Err(AuthError::Unauthorized);
}

struct HeaderTokens<'a> {
  auth_token: &'a str,
  refresh_token: Option<&'a str>,
}

#[inline]
fn extract_tokens_from_headers<'a>(headers: &'a header::HeaderMap) -> Option<HeaderTokens<'a>> {
  let auth_token =
    get_header(headers, header::AUTHORIZATION).and_then(|v| v.strip_prefix("Bearer "))?;
  let refresh_token = get_header(headers, HEADER_REFRESH_TOKEN);

  return Some(HeaderTokens {
    auth_token,
    refresh_token,
  });
}

struct CookieTokens {
  auth_token: Option<String>,
  refresh_token: Option<String>,
}

#[inline]
fn extract_tokens_from_cookies(cookies: &Cookies) -> Option<CookieTokens> {
  let auth_token = cookies
    .get(COOKIE_AUTH_TOKEN)
    .as_ref()
    .map(|c| c.value().to_owned());
  let refresh_token = cookies
    .get(COOKIE_REFRESH_TOKEN)
    .as_ref()
    .map(|c| c.value().to_owned());

  return match (auth_token, refresh_token) {
    (None, None) => None,
    (auth_token, refresh_token) => Some(CookieTokens {
      auth_token,
      refresh_token,
    }),
  };
}

/// Only difference to Tokens above, refresh token presence is guaranteed.
pub struct FreshTokens {
  pub auth_token_claims: (AuthTokenClaims, Duration),
  pub refresh_token: Option<(String, Duration)>,
}

pub(crate) async fn mint_new_tokens(
  state: &AppState,
  db_user_and_tenant: &DbUserAndTenant,
  auth_and_refresh_token_ttl: Option<(Duration, Duration)>,
) -> Result<FreshTokens, AuthError> {
  let DbUserAndTenant { user, tenant } = db_user_and_tenant;

  if user.unverified_email.is_some() {
    return Err(AuthError::Internal(
      "Cannot mint tokens for unverified user".into(),
    ));
  }

  let (auth_token_ttl, refresh_token_ttl) =
    auth_and_refresh_token_ttl.unwrap_or_else(|| state.access_config(|c| c.auth.token_ttls()));

  let refresh_token = if refresh_token_ttl.num_seconds() > 0 {
    // Unlike the JWT auth tokens, refresh tokens are opaque.
    let refresh_token = random_alphanumeric(REFRESH_TOKEN_LENGTH);
    let expires = (chrono::Utc::now() + refresh_token_ttl).timestamp();

    const SESSION_INSERT_QUERY: &str = formatcp!(
      "INSERT INTO \"{SESSION_TABLE}\" (user, tenant, refresh_token, expires) VALUES ($1, $2, $3, $4)"
    );

    state
      .session_conn()
      .execute(
        SESSION_INSERT_QUERY,
        params!(
          user.id,
          tenant.map(|t| t.to_vec()),
          refresh_token.clone(),
          expires
        ),
      )
      .await?;

    Some((refresh_token, refresh_token_ttl))
  } else {
    None
  };

  return Ok(FreshTokens {
    auth_token_claims: (
      AuthTokenClaims::new(&db_user_and_tenant, &auth_token_ttl),
      auth_token_ttl,
    ),
    refresh_token,
  });
}

pub(crate) async fn reauth_with_refresh_token(
  state: &AppState,
  refresh_token: String,
) -> Result<(AuthTokenClaims, chrono::Duration), AuthError> {
  let (auth_token_ttl, _refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());

  // QUESTION: Should we push expiry on refresh? At this point refresh token TTL would only start
  // ticking with last login. This may be especially useful for anonymous auth.
  const SESSION_QUERY: &str = formatcp!(
    "\
      SELECT user, tenant \
      FROM '{SESSION_TABLE}' \
      WHERE \
        refresh_token = $1 AND expires > UNIXEPOCH() \
    "
  );

  let Some((user_id, tenant_id)) = state
    .session_conn()
    .call_reader(|mut conn| -> Result<_, trailbase_sqlite::Error> {
      let row = conn.query_row(SESSION_QUERY, params!(refresh_token))?;
      let Some(row) = row else {
        return Ok(None);
      };

      return Ok(Some((
        row.get::<[u8; 16]>(0)?,
        row.get::<Option<[u8; 16]>>(1)?,
      )));
    })
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

  let db_user_and_tenant = if let Some(tenant_id) = tenant_id {
    const USER_AND_TENANT_QUERY: &str = formatcp!(
      "\
        SELECT {columns}, mem.tenant FROM \"{USER_TABLE}\" \
          RIGHT JOIN _membership AS mem ON mem.user = id \
        WHERE \
          id = $1 AND \
          unverified_email IS NULL AND \
          mem.tenant = $2 \
      ",
      columns = DbUser::COLUMNS
    );

    state
      .user_conn()
      .read_query_row(USER_AND_TENANT_QUERY, params!(user_id, tenant_id))
      .await?
      .map(DbUserAndTenant::from_row)
      .transpose()?
      .ok_or_else(|| {
        #[cfg(debug_assertions)]
        log::debug!("User not found or unverified");

        return AuthError::Unauthorized;
      })?
  } else {
    const USER_QUERY: &str = formatcp!(
      r#"SELECT {columns} FROM "{USER_TABLE}" WHERE id = $1 AND unverified_email IS NULL"#,
      columns = DbUser::COLUMNS
    );

    DbUserAndTenant {
      tenant: None,
      user: state
        .user_conn()
        .read_query_row(USER_QUERY, params!(user_id))
        .await?
        .map(DbUser::from_row)
        .transpose()?
        .ok_or_else(|| {
          #[cfg(debug_assertions)]
          log::debug!("User not found or unverified");

          return AuthError::Unauthorized;
        })?,
    }
  };

  debug_assert!(
    db_user_and_tenant.user.unverified_email.is_none(),
    "unverified user, should have been caught by query above"
  );

  return Ok((
    AuthTokenClaims::new(&db_user_and_tenant, &auth_token_ttl),
    auth_token_ttl,
  ));
}
