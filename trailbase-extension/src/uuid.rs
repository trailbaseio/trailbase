use rusqlite::functions::Context;
use rusqlite::types::ValueRef;
use rusqlite::Error;
use uuid::Uuid;

/// Checks that argument is a valid UUID blob or null.
///
/// Null is explicitly allowed to enable use as CHECK constraint in nullable columns.
pub(super) fn is_uuid(context: &Context) -> rusqlite::Result<bool> {
  return Ok(unpack_uuid_or_null(context).is_ok());
}

/// Checks that argument is a valid UUIDv7 blob or null.
///
/// Null is explicitly allowed to enable use as CHECK constraint in nullable columns.
pub(super) fn is_uuid_v7(context: &Context) -> rusqlite::Result<bool> {
  return Ok(match unpack_uuid_or_null(context)? {
    Some(uuid) => uuid.get_version_num() == 7,
    None => true,
  });
}

/// Creates a new UUIDv7 blob.
pub(super) fn uuid_v7(_context: &Context) -> rusqlite::Result<Vec<u8>> {
  return Ok(Uuid::now_v7().as_bytes().to_vec());
}

/// Format UUID blob as string-encoded UUID.
pub(super) fn uuid_text(context: &Context) -> rusqlite::Result<String> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return match context.get_raw(0) {
    ValueRef::Blob(blob) => {
      let uuid = Uuid::from_slice(blob).map_err(|err| Error::UserFunctionError(err.into()))?;
      Ok(uuid.to_string())
    }
    arg => Err(Error::UserFunctionError(
      format!("Expected UUID blob, got {}", arg.data_type()).into(),
    )),
  };
}

/// Parse UUID from string-encoded UUID.
pub(super) fn uuid_parse(context: &Context) -> rusqlite::Result<Vec<u8>> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return match context.get_raw(0) {
    ValueRef::Text(ascii) => {
      let uuid =
        Uuid::try_parse_ascii(ascii).map_err(|err| Error::UserFunctionError(err.into()))?;

      Ok(uuid.as_bytes().to_vec())
    }
    arg => Err(Error::UserFunctionError(
      format!("Expected text, got {}", arg.data_type()).into(),
    )),
  };
}

#[inline]
fn unpack_uuid_or_null(context: &Context<'_>) -> rusqlite::Result<Option<Uuid>> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return match context.get_raw(0) {
    ValueRef::Null => Ok(None),
    ValueRef::Blob(blob) => {
      let uuid = Uuid::from_slice(blob).map_err(|err| Error::UserFunctionError(err.into()))?;
      Ok(Some(uuid))
    }
    _ => Err(Error::UserFunctionError(
      "Expected BLOB column type.".into(),
    )),
  };
}

#[cfg(test)]
mod tests {
  use rusqlite::params;
  use uuid::Uuid;

  #[test]
  fn test_uuid() {
    let conn = crate::connect_sqlite(None, None).unwrap();

    let create_table = r#"
        CREATE TABLE test (
          id                           BLOB PRIMARY KEY NOT NULL DEFAULT (uuid_v7()),
          uuid                         BLOB CHECK(is_uuid(uuid)),
          uuid_v7                      BLOB CHECK(is_uuid_v7(uuid_v7))
        ) STRICT;
      "#;
    conn.execute(create_table, ()).unwrap();

    {
      let row = conn
        .query_row(
          "INSERT INTO test (uuid, uuid_v7) VALUES (NULL, NULL) RETURNING id",
          (),
          |row| -> rusqlite::Result<[u8; 16]> { Ok(row.get(0)?) },
        )
        .unwrap();

      Uuid::from_slice(&row).unwrap();
    }

    {
      assert!(conn
        .execute(
          "INSERT INTO test (uuid, uuid_v7) VALUES ($1, NULL)",
          params!(b"")
        )
        .is_err());
    }

    {
      assert!(conn
        .execute(
          "INSERT INTO test (uuid, uuid_v7) VALUES (NULL, $1)",
          params!(Vec::<u8>::from([0, 0, 1, 2, 3, 4, 5, 6]))
        )
        .is_err());
    }

    {
      let uuid = Uuid::now_v7();
      let row = conn
        .query_row(
          "INSERT INTO test (uuid, uuid_v7) VALUES (uuid_parse($1), uuid_parse($1)) RETURNING uuid",
          [uuid.to_string()],
          |row| -> rusqlite::Result<[u8; 16]> { Ok(row.get(0)?) },
        )
        .unwrap();

      assert_eq!(Uuid::from_slice(&row).unwrap(), uuid);
    }

    {
      let row = conn
        .query_row(
          "SELECT uuid_parse(uuid_text(uuid_v7()))",
          [],
          |row| -> rusqlite::Result<[u8; 16]> { Ok(row.get(0)?) },
        )
        .unwrap();

      assert_eq!(Uuid::from_slice(&row).unwrap().get_version_num(), 7);
    }
  }
}
