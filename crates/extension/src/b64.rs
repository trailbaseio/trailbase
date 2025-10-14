use base64::prelude::*;
use rusqlite::functions::Context;

/// Format blob as string-encoded URL-safe base64 string.
pub(super) fn b64_text(context: &Context) -> Result<String, Error> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  let blob = context.get_raw(0).as_blob()?;
  return Ok(BASE64_URL_SAFE.encode(blob));
}

/// Parse blob from URL-safe base64 string.
pub(super) fn b64_parse(context: &Context) -> Result<Vec<u8>, Error> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  let str = context.get_raw(0).as_str()?;

  return Ok(BASE64_URL_SAFE
    .decode(str)
    .map_err(|err| Error::UserFunctionError(err.into()))?);
}

#[cfg(test)]
mod tests {
  use rusqlite::Error;
  use uuid::Uuid;

  #[test]
  fn test_b64() {
    let conn = crate::connect_sqlite(None).unwrap();

    let row = conn
      .query_row(
        "SELECT b64_parse(b64_text(uuid_v7()))",
        [],
        |row| -> Result<[u8; 16], Error> { Ok(row.get(0)?) },
      )
      .unwrap();

    assert_eq!(Uuid::from_slice(&row).unwrap().get_version_num(), 7);
  }
}
