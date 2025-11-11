use base64::prelude::*;
use rusqlite::Error;
use rusqlite::Result;
use rusqlite::functions::Context;
use rusqlite::types::{Value, ValueRef};

/// A base64 conversion utility similar to the SQLite's `base64()` extension.
///
/// It introspects on the inputs, and will convert based on that:
/// - BLOB → TEXT (encode)
/// - TEXT → BLOB (decode)
/// - NULL → NULL
/// - Other types → error
///
/// Note, however, that this implementation is more strict with respect to b64
/// text inputs, e.g. it checks the padding.
pub(super) fn base64(context: &Context) -> Result<Value> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return match context.get_raw(0) {
    ValueRef::Null => Ok(Value::Null),
    ValueRef::Blob(blob) => Ok(Value::Text(BASE64_STANDARD.encode(blob))),
    ValueRef::Text(text_bytes) => {
      let text_str =
        std::str::from_utf8(text_bytes).map_err(|err| Error::UserFunctionError(err.into()))?;

      Ok(Value::Blob(
        BASE64_STANDARD
          .decode(text_str)
          .map_err(|err| Error::UserFunctionError(err.into()))?,
      ))
    }
    v => Err(Error::InvalidFunctionParameterType(0, v.data_type())),
  };
}

/// A URL-safe base64 conversion utility similar to the SQLite's `base64()` extension.
///
/// It introspects on the inputs, and will convert based on that:
/// - BLOB → TEXT (encode)
/// - TEXT → BLOB (decode)
/// - NULL → NULL
/// - Other types → error
///
/// Note, however, that this implementation is more strict with respect to b64
/// text inputs, e.g. it checks the padding.
pub(super) fn base64_url_safe(context: &Context) -> Result<Value> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return match context.get_raw(0) {
    ValueRef::Null => Ok(Value::Null),
    ValueRef::Blob(blob) => Ok(Value::Text(BASE64_URL_SAFE.encode(blob))),
    ValueRef::Text(text_bytes) => {
      let text_str =
        std::str::from_utf8(text_bytes).map_err(|err| Error::UserFunctionError(err.into()))?;

      Ok(Value::Blob(
        BASE64_URL_SAFE
          .decode(text_str)
          .map_err(|err| Error::UserFunctionError(err.into()))?,
      ))
    }
    v => Err(Error::InvalidFunctionParameterType(0, v.data_type())),
  };
}

#[cfg(test)]
mod tests {
  use base64::prelude::*;
  use rusqlite::Error;

  #[test]
  fn test_base64_wrong_number_of_arguments() {
    let conn = crate::connect_sqlite(None, None).unwrap();
    let val = conn.query_row(
      "SELECT base64_url_safe('a', 'b')",
      [],
      |row| -> Result<[u8; 16], Error> { Ok(row.get(0)?) },
    );
    assert!(val.is_err());
  }

  #[test]
  fn test_base64_url_safe_roundtrip() {
    let conn = crate::connect_sqlite(None, None).unwrap();

    let value = b"832!@#$%^&*()>./";
    for query in [
      format!("SELECT base64(base64(?1))"),
      format!("SELECT base64_url_safe(base64_url_safe(?1))"),
    ] {
      // BLOB → TEXT → BLOB round-trip test
      let val = conn
        .query_row(&query, [value], |row| -> Result<Vec<u8>, Error> {
          Ok(row.get(0)?)
        })
        .unwrap();
      assert_eq!(val, value);
    }
  }

  #[test]
  fn test_base64_url_safe_null_handling() {
    let conn = crate::connect_sqlite(None, None).unwrap();
    for query in [
      format!("SELECT base64(NULL)"),
      format!("SELECT base64_url_safe(NULL)"),
    ] {
      let val: Option<Vec<u8>> = conn.query_row(&query, [], |row| row.get(0)).unwrap();
      assert!(val.is_none());
    }
  }

  #[test]
  fn test_base64_url_safe_empty_string() {
    let conn = crate::connect_sqlite(None, None).unwrap();
    for query in [
      format!("SELECT base64('')"),
      format!("SELECT base64_url_safe('')"),
    ] {
      let val: Vec<u8> = conn.query_row(&query, [], |row| row.get(0)).unwrap();
      assert!(val.is_empty());
    }
  }

  #[test]
  fn test_base64_url_safe_trimmed_input() {
    let conn = crate::connect_sqlite(None, None).unwrap();
    let encoded = BASE64_URL_SAFE.encode(&[1, 2, 3, 4]);

    for query in [
      format!("SELECT base64('  {encoded}   ')"),
      format!("SELECT base64_url_safe('  {encoded}   ')"),
    ] {
      let v = conn.query_row(&query, [], |row| row.get::<_, Vec<u8>>(0));
      assert!(v.is_err());
    }

    for query in [
      format!("SELECT base64(trim('  {encoded}   '))"),
      format!("SELECT base64_url_safe(trim('  {encoded}   '))"),
    ] {
      let val: Vec<u8> = conn.query_row(&query, [], |row| row.get(0)).unwrap();
      assert_eq!(val, vec![1, 2, 3, 4]);
    }
  }
}
