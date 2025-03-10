use base64::prelude::*;
use chrono::Duration;
use lazy_static::lazy_static;
use sha2::{Digest, Sha256};
use tower_cookies::{
  cookie::{self, SameSite},
  Cookie, Cookies,
};
use trailbase_sqlite::params;

use crate::auth::user::{DbUser, User};
use crate::auth::AuthError;
use crate::constants::{
  COOKIE_AUTH_TOKEN, COOKIE_OAUTH_STATE, COOKIE_REFRESH_TOKEN, SESSION_TABLE, USER_TABLE,
};
use crate::AppState;

pub(crate) fn validate_redirects(
  state: &AppState,
  first: &Option<String>,
  second: &Option<String>,
) -> Result<Option<String>, AuthError> {
  let site = state.access_config(|c| c.server.site_url.clone());

  let valid = |redirect: &String| -> bool {
    if redirect.starts_with("/") {
      return true;
    }
    if state.dev_mode() && redirect.starts_with("http://localhost") {
      return true;
    }

    // TODO: Add a configurable allow list.
    if let Some(site) = site {
      return redirect.starts_with(&site);
    }
    return false;
  };

  #[allow(clippy::manual_flatten)]
  for r in [first, second] {
    if let Some(r) = r {
      if valid(r) {
        return Ok(Some(r.to_owned()));
      }
      return Err(AuthError::BadRequest("Invalid redirect"));
    }
  }

  return Ok(None);
}

pub(crate) fn new_cookie(
  key: &'static str,
  value: String,
  ttl: Duration,
  dev: bool,
) -> Cookie<'static> {
  return Cookie::build((key, value))
    .path("/")
    // Not available to client-side JS.
    .http_only(true)
    // Only send cookie over HTTPs.
    .secure(!dev)
    // Only include cookie if request originates from origin site.
    .same_site(if dev { SameSite::Lax } else { SameSite::Strict })
    .max_age(cookie::time::Duration::seconds(ttl.num_seconds()))
    .build();
}

pub(crate) fn new_cookie_opts(
  key: &'static str,
  value: String,
  ttl: Duration,
  tls_only: bool,
  same_site: bool,
) -> Cookie<'static> {
  return Cookie::build((key, value))
    .path("/")
    // Not available to client-side JS.
    .http_only(true)
    // Only send cookie over HTTPs.
    .secure(tls_only)
    // Only include cookie if request originates from origin site.
    .same_site(if same_site {
      SameSite::Strict
    } else {
      SameSite::Lax
    })
    .max_age(cookie::time::Duration::seconds(ttl.num_seconds()))
    .build();
}

/// Removes cookie with the given `key`.
///
/// NOTE: Removing a cookie from the jar doesn't reliably force the browser to remove the cookie,
/// thus override them.
pub(crate) fn remove_cookie(cookies: &Cookies, key: &'static str) {
  if cookies.get(key).is_some() {
    cookies.add(new_cookie(key, "".to_string(), Duration::seconds(1), false));
  }
}

pub(crate) fn remove_all_cookies(cookies: &Cookies) {
  for cookie in [COOKIE_AUTH_TOKEN, COOKIE_REFRESH_TOKEN, COOKIE_OAUTH_STATE] {
    remove_cookie(cookies, cookie);
  }
}

pub async fn user_by_email(state: &AppState, email: &str) -> Result<DbUser, AuthError> {
  return get_user_by_email(state.user_conn(), email).await;
}

pub async fn get_user_by_email(
  user_conn: &trailbase_sqlite::Connection,
  email: &str,
) -> Result<DbUser, AuthError> {
  lazy_static! {
    static ref QUERY: String = format!(r#"SELECT * FROM "{USER_TABLE}" WHERE email = $1"#);
  };
  let db_user = user_conn
    .query_value::<DbUser>(&QUERY, params!(email.to_string()))
    .await
    .map_err(|_err| AuthError::UnauthorizedExt("user not found by email".into()))?;

  return db_user.ok_or_else(|| AuthError::UnauthorizedExt("invalid user".into()));
}

pub async fn user_by_id(state: &AppState, id: &uuid::Uuid) -> Result<DbUser, AuthError> {
  return get_user_by_id(state.user_conn(), id).await;
}

async fn get_user_by_id(
  user_conn: &trailbase_sqlite::Connection,
  id: &uuid::Uuid,
) -> Result<DbUser, AuthError> {
  lazy_static! {
    static ref QUERY: String = format!(r#"SELECT * FROM "{USER_TABLE}" WHERE id = $1"#);
  };
  let db_user = user_conn
    .query_value::<DbUser>(&QUERY, params!(id.into_bytes()))
    .await
    .map_err(|_err| AuthError::UnauthorizedExt("User not found by id".into()))?;

  return db_user.ok_or_else(|| AuthError::UnauthorizedExt("invalid user".into()));
}

pub async fn user_exists(state: &AppState, email: &str) -> Result<bool, AuthError> {
  lazy_static! {
    static ref QUERY: String =
      format!(r#"SELECT EXISTS(SELECT 1 FROM "{USER_TABLE}" WHERE email = $1)"#);
  };
  return Ok(
    state
      .user_conn()
      .query_value(&QUERY, params!(email.to_string()))
      .await?
      .ok_or_else(|| AuthError::Internal("query should return".into()))?,
  );
}

pub(crate) async fn is_admin(state: &AppState, user: &User) -> bool {
  lazy_static! {
    static ref QUERY: String = format!(r#"SELECT admin FROM "{USER_TABLE}" WHERE id = $1"#);
  };

  let Ok(Some(row)) = state
    .user_conn()
    .query_row(&QUERY, params!(user.uuid.as_bytes().to_vec()))
    .await
  else {
    return false;
  };

  return row.get::<bool>(0).unwrap_or(false);
}

pub(crate) async fn delete_all_sessions_for_user(
  state: &AppState,
  user_id: uuid::Uuid,
) -> Result<usize, AuthError> {
  lazy_static! {
    static ref QUERY: String = format!(r#"DELETE FROM "{SESSION_TABLE}" WHERE user = $1"#);
  };

  return Ok(
    state
      .user_conn()
      .execute(
        &QUERY,
        [trailbase_sqlite::Value::Blob(user_id.into_bytes().to_vec())],
      )
      .await?,
  );
}

pub(crate) async fn delete_session(
  state: &AppState,
  refresh_token: String,
) -> Result<usize, AuthError> {
  lazy_static! {
    static ref QUERY: String = format!(r#"DELETE FROM "{SESSION_TABLE}" WHERE refresh_token = $1"#);
  };

  return Ok(
    state
      .user_conn()
      .execute(&QUERY, params!(refresh_token))
      .await?,
  );
}

/// Derives the code challenge given the verifier as base64UrlNoPad(sha256([codeVerifier])).
///
/// NOTE: We could also use oauth2::PkceCodeChallenge.
pub(crate) fn derive_pkce_code_challenge(pkce_code_verifier: &str) -> String {
  let mut sha = Sha256::new();
  sha.update(pkce_code_verifier);
  // NOTE: This is NO_PAD as per the spec.
  return BASE64_URL_SAFE_NO_PAD.encode(sha.finalize());
}
