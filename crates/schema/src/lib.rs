#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

// This crate became a kitchen sink for three different kinds of "schemas": DB schemas (e.g.
// Sqlite), general JSON work (e.g. turning DB values into JSON records), and JSONSchema(TM) stuff.
pub mod db;
pub mod json;
pub mod json_schema;

// TODO: Clean up legacy re-exports after breaking up into 3 modules.
pub use db::sqlite::QualifiedName;

#[derive(Clone, Debug, thiserror::Error)]
pub enum IdError {
  #[error("Length: {0}")]
  InvalidLength(usize),
  #[error("Base64: {0}")]
  Base64(#[from] base64::DecodeError),
  #[error("Uuid: {0}")]
  Uuid(#[from] uuid::Error),
}

#[inline]
pub fn parse_uuid_vec(s: &str) -> Result<Vec<u8>, IdError> {
  use base64::prelude::*;

  let s = s.trim();
  return match s.len() {
    36 => Ok(uuid::Uuid::parse_str(s)?.into()),
    22 => Ok(BASE64_URL_SAFE_NO_PAD.decode(s)?),
    24 => Ok(BASE64_URL_SAFE.decode(s)?),
    len => Err(IdError::InvalidLength(len)),
  };
}

#[inline]
pub fn parse_uuid(s: &str) -> Result<uuid::Uuid, IdError> {
  use base64::prelude::*;

  let s = s.trim();
  return match s.len() {
    36 => Ok(uuid::Uuid::parse_str(s)?),
    22 => Ok(uuid::Uuid::from_slice(&BASE64_URL_SAFE_NO_PAD.decode(s)?)?),
    24 => Ok(uuid::Uuid::from_slice(&BASE64_URL_SAFE.decode(s)?)?),
    len => Err(IdError::InvalidLength(len)),
  };
}

#[cfg(test)]
mod tests {
  use base64::prelude::*;
  use uuid::Uuid;

  use super::*;

  #[test]
  fn test_parse_uuid() {
    let uuid = Uuid::new_v4();

    assert_eq!(uuid, parse_uuid(&format!("   {uuid} ")).unwrap());
    assert_eq!(
      uuid,
      parse_uuid(&BASE64_URL_SAFE.encode(uuid.as_bytes())).unwrap()
    );
    assert_eq!(
      uuid,
      parse_uuid(&BASE64_URL_SAFE_NO_PAD.encode(uuid.as_bytes())).unwrap()
    );

    assert_eq!(
      uuid.as_bytes(),
      parse_uuid_vec(&format!("   {uuid} ")).unwrap().as_slice()
    );
    assert_eq!(
      uuid.as_bytes(),
      parse_uuid_vec(&BASE64_URL_SAFE.encode(uuid.as_bytes()))
        .unwrap()
        .as_slice()
    );
    assert_eq!(
      uuid.as_bytes(),
      parse_uuid_vec(&BASE64_URL_SAFE_NO_PAD.encode(uuid.as_bytes()))
        .unwrap()
        .as_slice()
    );
  }
}
