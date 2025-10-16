use base64::prelude::*;
use rusqlite::Error;
use rusqlite::functions::Context;
use rusqlite::types::ValueRef;

/// Convert between BLOB and URL-safe base64 TEXT bidirectionally
pub(super) fn base64_url_safe(context: &Context) -> rusqlite::Result<rusqlite::types::Value> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  match context.get_raw(0) {
    // NULL → NULL
    ValueRef::Null => Ok(rusqlite::types::Value::Null),

    // BLOB → TEXT (encode URL-safe base64)
    ValueRef::Blob(blob) => {
      Ok(rusqlite::types::Value::Text(BASE64_URL_SAFE.encode(blob)))
    }

    // TEXT → BLOB (decode URL-safe base64)
    ValueRef::Text(text) => {
      let text_str = std::str::from_utf8(text)
        .map(|s| s.trim())
        .map_err(|err| Error::UserFunctionError(err.into()))?;

      let decoded = BASE64_URL_SAFE
        .decode(text_str)
        .map_err(|err| Error::UserFunctionError(err.into()))?;

      Ok(rusqlite::types::Value::Blob(decoded))
    }

    // OTHER VALUES → Error
    _ => Err(Error::InvalidFunctionParameterType(
      0,
      rusqlite::types::Type::Null,
    )),
  }
}

#[cfg(test)]
mod tests {
  use rusqlite::Error;
  use uuid::Uuid;

  #[test]
  fn test_base64_url_safe() {
    let conn = crate::connect_sqlite(None).unwrap();

    // BLOB → TEXT → BLOB roundtrip
    let row = conn
      .query_row(
        "SELECT base64_url_safe(base64_url_safe(uuid_v7()))",
        [],
        |row| -> Result<[u8; 16], Error> { Ok(row.get(0)?) },
      )
      .unwrap();
    assert_eq!(Uuid::from_slice(&row).unwrap().get_version_num(), 7);

    // Test NULL handling
    let null_result: Option<Vec<u8>> = conn
      .query_row("SELECT base64_url_safe(NULL)", [], |row| row.get(0))
      .unwrap();
    assert!(null_result.is_none());
  }
}
