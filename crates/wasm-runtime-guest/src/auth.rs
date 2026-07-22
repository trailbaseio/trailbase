use http::StatusCode;

use crate::db::{self, Value};
use crate::http::{HttpError, Request};

const CSRF_HEADER: &str = "CSRF-Token";

pub async fn require_admin(req: &Request) -> Result<(), HttpError> {
  let Some(user) = req.user() else {
    return Err(HttpError::status(StatusCode::UNAUTHORIZED));
  };

  if !is_admin(&user.id).await? {
    return Err(HttpError::status(StatusCode::FORBIDDEN));
  }

  if *req.method() != http::Method::GET {
    let received = req.header(CSRF_HEADER).and_then(|v| v.to_str().ok());
    if received != Some(user.csrf_token.as_str()) {
      return Err(HttpError::status(StatusCode::FORBIDDEN));
    }
  }

  Ok(())
}

pub async fn is_admin(user_id: &str) -> Result<bool, HttpError> {
  use base64::engine::DecodePaddingMode;
  use base64::engine::general_purpose::GeneralPurpose;
  use base64::{Engine, alphabet, engine::general_purpose::GeneralPurposeConfig};

  let engine = GeneralPurpose::new(
    &alphabet::URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
  );

  let id_bytes = engine.decode(user_id.as_bytes()).map_err(|err| {
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

  match rows.first().and_then(|row| row.first()) {
    Some(Value::Integer(1)) => Ok(true),
    Some(Value::Integer(0)) => Ok(false),
    _ => Err(HttpError::status(StatusCode::FORBIDDEN)),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn csrf_header_constant_matches_host() {
    assert_eq!(CSRF_HEADER, "CSRF-Token");
  }
}
