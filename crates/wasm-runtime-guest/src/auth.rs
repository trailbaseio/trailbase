use base64::engine::general_purpose::URL_SAFE_PAD_INDIFFERENT;
use http::{HeaderValue, StatusCode};
pub use trailbase_wasm_common::HttpContextUser as User;

use crate::db::{self, Value};
use crate::http::{HttpError, Request};

pub async fn require_admin(req: &Request) -> Result<(), HttpError> {
  return require_admin_impl(req.user(), req.method(), req.header(CSRF_HEADER)).await;
}

#[inline]
pub async fn require_admin_impl(
  user: Option<&User>,
  method: &http::Method,
  csrf_token: Option<&HeaderValue>,
) -> Result<(), HttpError> {
  let Some(user) = user else {
    return Err(HttpError::status(StatusCode::UNAUTHORIZED));
  };

  if !is_admin(&user).await? {
    return Err(HttpError::status(StatusCode::FORBIDDEN));
  }

  if method != http::Method::GET {
    let received = csrf_token.and_then(|v| v.to_str().ok());
    if received != Some(user.csrf_token.as_str()) {
      return Err(HttpError::status(StatusCode::FORBIDDEN));
    }
  }

  return Ok(());
}

pub async fn is_admin(user: &User) -> Result<bool, HttpError> {
  use base64::prelude::*;

  let id_bytes = URL_SAFE_PAD_INDIFFERENT
    .decode(user.id.as_bytes())
    .map_err(|err| {
      log::warn!("require_admin: invalid user id encoding: {err}");
      HttpError::status(StatusCode::INTERNAL_SERVER_ERROR)
    })?;

  let rows = db::query(
    r#"SELECT admin FROM "_user" WHERE id = ?"#,
    vec![Value::Blob(id_bytes)],
  )
  .await
  .map_err(|err| {
    log::warn!("require_admin: db query failed: {err}");
    HttpError::status(StatusCode::INTERNAL_SERVER_ERROR)
  })?;

  return match rows.first().and_then(|row| row.first()) {
    Some(Value::Integer(v)) if *v > 0 => Ok(true),
    Some(Value::Integer(_)) => Ok(false),
    _ => Err(HttpError::status(StatusCode::FORBIDDEN)),
  };
}

pub(crate) const CSRF_HEADER: &str = "CSRF-Token";

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn csrf_header_constant_matches_host() {
    assert_eq!(CSRF_HEADER, "CSRF-Token");
  }
}
