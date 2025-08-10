use rusqlite::Error;
use rusqlite::functions::Context;
use uuid::Uuid;

/// Checks that argument is a valid UUID blob or null.
///
/// Null is explicitly allowed to enable use as CHECK constraint in nullable columns.
pub(super) fn is_uuid(context: &Context) -> Result<bool, Error> {
  return Ok(unpack_uuid_or_null(context).is_ok());
}

/// Checks that argument is a valid UUIDv4 blob or null.
///
/// Null is explicitly allowed to enable use as CHECK constraint in nullable columns.
pub(super) fn is_uuid_v4(context: &Context) -> Result<bool, Error> {
  let Some(uuid) = unpack_uuid_or_null(context)? else {
    return Ok(true);
  };

  return Ok(uuid.get_version_num() == 7);
}

/// Creates a new UUIDv4 blob.
pub(super) fn uuid_v4(_context: &Context) -> Result<[u8; 16], Error> {
  return Ok(Uuid::new_v4().into_bytes());
}

/// Checks that argument is a valid UUIDv7 blob or null.
///
/// Null is explicitly allowed to enable use as CHECK constraint in nullable columns.
pub(super) fn is_uuid_v7(context: &Context) -> Result<bool, Error> {
  let Some(uuid) = unpack_uuid_or_null(context)? else {
    return Ok(true);
  };

  return Ok(uuid.get_version_num() == 7);
}

/// Creates a new UUIDv7 blob.
pub(super) fn uuid_v7(_context: &Context) -> Result<[u8; 16], Error> {
  return Ok(Uuid::now_v7().into_bytes());
}

/// Format UUID blob as string-encoded UUID.
pub(super) fn uuid_text(context: &Context) -> Result<String, Error> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  let blob = context.get_raw(0).as_blob()?;
  let uuid = Uuid::from_slice(blob).map_err(|err| Error::UserFunctionError(err.into()))?;
  return Ok(uuid.to_string());
}

/// Parse UUID from string-encoded UUID.
pub(super) fn uuid_parse(context: &Context) -> Result<[u8; 16], Error> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  let str = context.get_raw(0).as_str()?;
  let uuid = Uuid::try_parse(str).map_err(|err| Error::UserFunctionError(err.into()))?;
  return Ok(uuid.into_bytes());
}

#[inline]
fn unpack_uuid_or_null(context: &Context<'_>) -> Result<Option<Uuid>, Error> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  if let Some(blob) = context.get_raw(0).as_blob_or_null()? {
    let uuid = Uuid::from_slice(blob).map_err(|err| Error::UserFunctionError(err.into()))?;
    return Ok(Some(uuid));
  }
  return Ok(None);
}

#[cfg(test)]
mod tests {
  use rusqlite::{Error, params};
  use uuid::Uuid;

  #[test]
  fn test_uuid() {
    let conn = crate::connect_sqlite(None).unwrap();

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
          |row| -> Result<[u8; 16], Error> { Ok(row.get(0)?) },
        )
        .unwrap();

      Uuid::from_slice(&row).unwrap();
    }

    {
      assert!(
        conn
          .execute(
            "INSERT INTO test (uuid, uuid_v7) VALUES ($1, NULL)",
            params!(b"")
          )
          .is_err()
      );
    }

    {
      assert!(
        conn
          .execute(
            "INSERT INTO test (uuid, uuid_v7) VALUES (NULL, $1)",
            params!(Vec::<u8>::from([0, 0, 1, 2, 3, 4, 5, 6]))
          )
          .is_err()
      );
    }

    {
      let uuid = Uuid::now_v7();
      let row = conn
        .query_row(
          "INSERT INTO test (uuid, uuid_v7) VALUES (uuid_parse($1), uuid_parse($1)) RETURNING uuid",
          [uuid.to_string()],
          |row| -> Result<[u8; 16], Error> { Ok(row.get(0)?) },
        )
        .unwrap();

      assert_eq!(Uuid::from_slice(&row).unwrap(), uuid);
    }

    {
      let row = conn
        .query_row(
          "SELECT uuid_parse(uuid_text(uuid_v7()))",
          [],
          |row| -> Result<[u8; 16], Error> { Ok(row.get(0)?) },
        )
        .unwrap();

      assert_eq!(Uuid::from_slice(&row).unwrap().get_version_num(), 7);
    }
  }
}
