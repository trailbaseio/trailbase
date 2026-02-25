use base64::prelude::*;
use chrono::Duration;
use const_format::formatcp;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;
use tower_cookies::{
  Cookie, Cookies,
  cookie::{self, SameSite},
};
use trailbase_sqlite::{Connection, params};
use validator::ValidateEmail;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::user::DbUser;
use crate::constants::{
  COOKIE_AUTH_TOKEN, COOKIE_OAUTH_STATE, COOKIE_REFRESH_TOKEN, SESSION_TABLE, USER_TABLE,
};

/// Strips plus-addressing, e.g. foo+spam@test.org becomes foo@test.org.
///
/// NOTE: We're not currently using this, see argument on `validate_and_normalize_email_address`.
#[allow(unused)]
fn strip_plus_email_addressing(email_address: &str) -> String {
  static PLUS_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[+].*?@").expect("covered by tests"));
  return PLUS_PATTERN.replace(email_address, "@").to_string();
}

/// Validates the given email addresses and returns a best-effort normalized address, .i.e. trim
/// whitespaces and lower-case conversion.
///
/// We're deliberately do not try to collapse equivalent email addresses, e.g., plus addressing
/// like "foo+spam@test.org". There's no robust way to do so, since different providers have
/// different rules, e.g. GMail strips all periods.
///
/// Even if we could, it would be rude to reply on an address different than what the user
/// provided, e.g. "foo+filter@test.org". Even if we did some equivalency checks, the original
/// address should remain untouched as the primary means of contact.
///
/// Moreover, email addresses are never robust abuse detection. If you know about "plus addressing"
/// you can also registers a domain and get infinite unique addresses or use burner email services.
/// Instead, for critical use-cases one should rely on stronger IDs such as phone numbers or better
/// photo ids.
pub fn validate_and_normalize_email_address(email_address: &str) -> Result<String, AuthError> {
  let email_address = email_address.trim().to_ascii_lowercase();
  if !email_address.validate_email() {
    return Err(AuthError::BadRequest("Invalid email"));
  }

  return Ok(email_address.to_string());
}

#[inline]
fn validate_redirect_impl(
  site: Option<&url::Url>,
  custom_uri_schemes: &[String],
  redirect: &str,
  dev: bool,
) -> Result<(), AuthError> {
  // Always accept redirects relative to current site.
  if redirect.starts_with("/") {
    return Ok(());
  }

  if let Ok(url) = url::Url::parse(redirect) {
    let scheme = url.scheme();
    if let Some(site) = site
      && url.host() == site.host()
      && scheme == site.scheme()
    {
      return Ok(());
    }

    // Allow custom schemes for mobile apps, desktop and SPAs.
    if scheme != "http" && scheme != "https" {
      for custom_scheme in custom_uri_schemes {
        if url.scheme() == custom_scheme {
          return Ok(());
        }
      }
    }

    if dev
      && match url.host() {
        Some(url::Host::Ipv4(ip)) if ip.is_loopback() => true,
        Some(url::Host::Ipv6(ip)) if ip.is_loopback() => true,
        Some(url::Host::Domain("localhost")) => true,
        _ => false,
      }
    {
      return Ok(());
    }
  }

  return Err(AuthError::BadRequest("invalid redirect"));
}

/// Validates up to two redirects, typically from query parameter and/or request body.
pub(crate) fn validate_redirect(
  state: &AppState,
  redirect_uri: Option<&str>,
) -> Result<(), AuthError> {
  if let Some(redirect_uri) = redirect_uri {
    let site: &Option<url::Url> = &state.site_url();
    let custom_uri_schemes = state.access_config(|c| c.auth.custom_uri_schemes.clone());

    validate_redirect_impl(
      site.as_ref(),
      &custom_uri_schemes,
      redirect_uri,
      state.dev_mode(),
    )?;
  }
  return Ok(());
}

pub async fn login_with_password_for_test(
  state: &AppState,
  normalized_email: &str,
  password: &str,
) -> Result<Option<crate::auth::api::login::NewTokens>, AuthError> {
  return crate::auth::api::login::login_with_password(
    state,
    normalized_email,
    password,
    None,
    state.access_config(|c| c.auth.token_ttls()).0,
  )
  .await;
}

#[cfg(test)]
pub async fn login_with_password(
  state: &AppState,
  normalized_email: &str,
  password: &str,
) -> Result<crate::auth::api::login::NewTokens, AuthError> {
  return Ok(
    login_with_password_for_test(state, normalized_email, password)
      .await?
      .unwrap(),
  );
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
  const QUERY: &str = formatcp!(r#"SELECT * FROM "{USER_TABLE}" WHERE email = $1"#);
  let db_user = user_conn
    .read_query_value::<DbUser>(QUERY, params!(email.to_string()))
    .await
    .map_err(|err| {
      debug_assert!(false, "GET USER BY EMAIL query failed: {err}");

      return AuthError::NotFound;
    })?;

  return db_user.ok_or_else(|| AuthError::NotFound);
}

pub async fn user_by_id(state: &AppState, id: &uuid::Uuid) -> Result<DbUser, AuthError> {
  return get_user_by_id(state.user_conn(), id).await;
}

pub async fn get_user_by_id(
  user_conn: &trailbase_sqlite::Connection,
  id: &uuid::Uuid,
) -> Result<DbUser, AuthError> {
  const QUERY: &str = formatcp!(r#"SELECT * FROM "{USER_TABLE}" WHERE id = $1"#);
  let db_user = user_conn
    .read_query_value::<DbUser>(QUERY, params!(id.into_bytes()))
    .await
    .map_err(|err| {
      debug_assert!(false, "GET USER BY ID query failed: {err}");

      return AuthError::NotFound;
    })?;

  return db_user.ok_or_else(|| AuthError::NotFound);
}

pub async fn user_exists(state: &AppState, email: &str) -> bool {
  const QUERY: &str = formatcp!(r#"SELECT EXISTS(SELECT 1 FROM "{USER_TABLE}" WHERE email = $1)"#);

  return match state
    .user_conn()
    .read_query_row_f(QUERY, params!(email.to_string()), |row| {
      row.get::<_, bool>(0)
    })
    .await
  {
    Ok(Some(row)) => row,
    Ok(None) => false,
    Err(err) => {
      debug_assert!(false, "USER EXISTS query failed: {err}");

      false
    }
  };
}

pub(crate) async fn is_admin(state: &AppState, user_id: &uuid::Uuid) -> bool {
  const QUERY: &str = formatcp!(r#"SELECT admin FROM "{USER_TABLE}" WHERE id = $1"#);

  return match state
    .user_conn()
    .read_query_row_f(QUERY, params!(user_id.as_bytes().to_vec()), |row| {
      row.get::<_, i64>(0)
    })
    .await
  {
    Ok(Some(row)) => row > 0,
    Ok(None) => false,
    Err(err) => {
      debug_assert!(false, "IS ADMIN query failed: {err}");

      false
    }
  };
}

pub(crate) async fn delete_all_sessions_for_user(
  user_conn: &Connection,
  user_id: uuid::Uuid,
) -> Result<usize, AuthError> {
  const QUERY: &str = formatcp!(r#"DELETE FROM "{SESSION_TABLE}" WHERE user = $1"#);

  return Ok(
    user_conn
      .execute(
        QUERY,
        [trailbase_sqlite::Value::Blob(user_id.into_bytes().to_vec())],
      )
      .await?,
  );
}

pub(crate) async fn delete_session(
  state: &AppState,
  refresh_token: String,
) -> Result<usize, AuthError> {
  const QUERY: &str = formatcp!(r#"DELETE FROM "{SESSION_TABLE}" WHERE refresh_token = $1"#);

  return state
    .user_conn()
    .execute(QUERY, params!(refresh_token))
    .await
    .map_err(|err| {
      debug_assert!(false, "DELETE SESSIONS query failed: {err}");

      return AuthError::Internal(err.into());
    });
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app_state::test_state;

  #[test]
  fn test_validate_email() {
    let normalized = validate_and_normalize_email_address(" fOO@test.org   ").unwrap();
    assert_eq!("foo@test.org", normalized);

    assert!(validate_and_normalize_email_address("foo!test.org").is_err());

    assert_eq!(
      strip_plus_email_addressing("foo+spam@test.org"),
      "foo@test.org"
    );
  }

  #[test]
  fn test_validate_redirect_impl() {
    let empty_site: Option<url::Url> = None;
    assert!(validate_redirect_impl(empty_site.as_ref(), &[], "", true).is_err());
    assert!(validate_redirect_impl(empty_site.as_ref(), &[], "/somewhere", false).is_ok());
    assert!(
      validate_redirect_impl(
        empty_site.as_ref(),
        &["custom".to_string()],
        "custom://somewhere",
        false
      )
      .is_ok()
    );
    assert!(validate_redirect_impl(empty_site.as_ref(), &[], "http://localhost", false).is_err());
    assert!(validate_redirect_impl(empty_site.as_ref(), &[], "http://127.0.0.1", false).is_err());
    assert!(validate_redirect_impl(empty_site.as_ref(), &[], "http://localhost", true).is_ok());

    let site = Some(url::Url::parse("https://test.org").unwrap());
    assert!(validate_redirect_impl(site.as_ref(), &[], "/somewhere", false).is_ok());
    assert!(
      validate_redirect_impl(site.as_ref(), &[], "https://test.org/somewhere", false).is_ok()
    );
    assert!(
      validate_redirect_impl(site.as_ref(), &[], "https://other.org/somewhere", false).is_err()
    );
    assert!(
      validate_redirect_impl(site.as_ref(), &[], "custom://test.org/somewhere", false).is_err()
    );
    assert!(
      validate_redirect_impl(
        site.as_ref(),
        &["custom".to_string()],
        "custom://test.org/somewhere",
        false
      )
      .is_ok()
    );
  }

  #[tokio::test]
  async fn test_validate_redirect() {
    let state = test_state(None).await.unwrap();

    assert!(validate_redirect(&state, None).is_ok());
    assert!(validate_redirect(&state, Some("invalid")).is_err());

    let redirect = "https://test.org";
    assert!(validate_redirect(&state, Some(redirect)).is_ok());
    assert!(validate_redirect(&state, Some("http://invalid.org")).is_err());

    for loopback in ["http://localhost", "http://127.0.0.1"] {
      assert!(validate_redirect(&state, Some(loopback)).is_ok());
    }

    assert!(validate_redirect(&state, Some("invalid://something")).is_err());
    let custom = "test-scheme://something";
    assert!(validate_redirect(&state, Some(custom)).is_ok());
  }
}
