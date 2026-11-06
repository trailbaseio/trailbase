use axum::http::HeaderMap;
use base64::prelude::*;
use reqwest::header::AsHeaderName;
use std::borrow::Cow;
use trailbase_sqlite::ConnectionType;
use uuid::Uuid;

pub use trailbase_schema::{IdError, parse_uuid};

#[inline]
pub fn row_id_column(connection_type: ConnectionType) -> &'static str {
  return match connection_type {
    ConnectionType::Pg => "ctid",
    ConnectionType::Sqlite => "_rowid_",
  };
}

pub fn id_to_b64(id: &[u8; 16]) -> String {
  return BASE64_URL_SAFE.encode(id);
}

pub fn uuid_to_b64(uuid: &Uuid) -> String {
  return BASE64_URL_SAFE.encode(uuid.as_bytes());
}

#[inline]
pub fn b64_to_uuid(b64_id: &str) -> Result<Uuid, IdError> {
  let s = b64_id.trim();
  return match s.len() {
    24 => Ok(Uuid::from_slice(&BASE64_URL_SAFE.decode(&s)?)?),
    len => Err(IdError::InvalidLength(len)),
  };
}

#[cfg(test)]
pub fn b64_to_id(b64_id: &str) -> Result<[u8; 16], IdError> {
  return Ok(b64_to_uuid(b64_id)?.into_bytes());
}

pub fn urlencode(s: &str) -> String {
  return form_urlencoded::byte_serialize(s.as_bytes()).collect();
}

#[inline]
pub(crate) fn get_header(headers: &HeaderMap, header_name: impl AsHeaderName) -> Option<&str> {
  if let Some(header) = headers.get(header_name) {
    return header.to_str().ok();
  }
  return None;
}

pub fn cow_to_string(cow: Cow<'static, [u8]>) -> String {
  match cow {
    Cow::Borrowed(x) => String::from_utf8_lossy(x).to_string(),
    Cow::Owned(x) => String::from_utf8_lossy(&x).to_string(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_urlencode() {
    assert_eq!(urlencode("+col0,-col1"), "%2Bcol0%2C-col1");
  }

  #[test]
  fn test_parse_uuid() {
    let uuid = Uuid::new_v4();

    assert_eq!(uuid, parse_uuid(&uuid.to_string()).unwrap());
    assert_eq!(
      uuid,
      parse_uuid(&BASE64_URL_SAFE.encode(uuid.as_bytes())).unwrap()
    );
    assert_eq!(
      uuid,
      parse_uuid(&BASE64_URL_SAFE_NO_PAD.encode(uuid.as_bytes())).unwrap()
    );
  }
}
