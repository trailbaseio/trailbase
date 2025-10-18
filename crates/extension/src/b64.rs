use base64::prelude::*;
use rusqlite::Error;
use rusqlite::Result;
use rusqlite::functions::Context;
use rusqlite::types::{Value, ValueRef};

/// A URL-safe base64 similar to the SQLite built-in `base64()` extension.
/// - BLOB → TEXT (encode)
/// - TEXT → BLOB (decode)
/// - NULL → NULL
/// - Other types → error
pub(super) fn base64_url_safe(context: &Context) -> Result<Value> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  match context.get_raw(0) {
    // NULL → NULL
    ValueRef::Null => Ok(Value::Null),

    // BLOB → TEXT (encode to URL-safe base64)
    ValueRef::Blob(blob) => Ok(Value::Text(BASE64_URL_SAFE.encode(blob))),

    // TEXT → BLOB (decode URL-safe base64)
    ValueRef::Text(text_bytes) => {
      // Trim whitespace like official base64()
      let text_str = std::str::from_utf8(text_bytes)
        .map_err(|err| Error::UserFunctionError(err.into()))?
        .trim();

      // Empty string should decode to empty blob (same as SQLite’s base64)
      if text_str.is_empty() {
        return Ok(Value::Blob(Vec::new()));
      }

      let decoded = BASE64_URL_SAFE
        .decode(text_str)
        .map_err(|err| Error::UserFunctionError(err.into()))?;

      Ok(Value::Blob(decoded))
    }

    // Other types → error
    _ => Err(Error::InvalidFunctionParameterType(
      0,
      context.get_raw(0).data_type(),
    )),
  }
}

#[cfg(test)]
mod tests {
  use base64::prelude::*;
  use rusqlite::Error;
  use uuid::Uuid;

  #[test]
  fn test_base64_url_safe_roundtrip() {
    let conn = crate::connect_sqlite(None).unwrap();

    // BLOB → TEXT → BLOB roundtrip test
    let val = conn
      .query_row(
        "SELECT base64_url_safe(base64_url_safe(uuid_v7()))",
        [],
        |row| -> Result<[u8; 16], Error> { Ok(row.get(0)?) },
      )
      .unwrap();
    assert_eq!(Uuid::from_slice(&val).unwrap().get_version_num(), 7);
  }

  #[test]
  fn test_base64_url_safe_null_handling() {
    let conn = crate::connect_sqlite(None).unwrap();
    let val: Option<Vec<u8>> = conn
      .query_row("SELECT base64_url_safe(NULL)", [], |row| row.get(0))
      .unwrap();
    assert!(val.is_none());
  }

  #[test]
  fn test_base64_url_safe_empty_string() {
    let conn = crate::connect_sqlite(None).unwrap();
    let val: Vec<u8> = conn
      .query_row("SELECT base64_url_safe('')", [], |row| row.get(0))
      .unwrap();
    assert!(val.is_empty());
  }

  #[test]
  fn test_base64_url_safe_trimmed_input() {
    let conn = crate::connect_sqlite(None).unwrap();
    let encoded = BASE64_URL_SAFE.encode([1u8, 2, 3, 4]);
    let val: Vec<u8> = conn
      .query_row(
        &format!("SELECT base64_url_safe('  {}  ')", encoded),
        [],
        |row| row.get(0),
      )
      .unwrap();
    assert_eq!(val, vec![1, 2, 3, 4]);
  }
}
