use rusqlite::Error;
use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::types::{FromSqlError, ValueRef};
use uuid::Uuid;

/// Checks that argument is a valid UUID blob or null.
///
/// Null is explicitly allowed to enable use as CHECK constraint in nullable columns.
fn is_uuid(context: &Context) -> Result<bool, Error> {
  return Ok(unpack_uuid_or_null(context).is_ok());
}

/// Checks that argument is a valid UUIDv4 blob or null.
///
/// Null is explicitly allowed to enable use as CHECK constraint in nullable columns.
fn is_uuid_v4(context: &Context) -> Result<bool, Error> {
  return match unpack_uuid_or_null(context) {
    Ok(Some(uuid)) => Ok(uuid.get_version_num() == 4),
    // Null returns true to allow nullable columns.
    Ok(None) => Ok(true),
    Err(_) => Ok(false),
  };
}

/// Checks that argument is a valid UUIDv7 blob or null.
///
/// Null is explicitly allowed to enable use as CHECK constraint in nullable columns.
fn is_uuid_v7(context: &Context) -> Result<bool, Error> {
  return match unpack_uuid_or_null(context) {
    Ok(Some(uuid)) => Ok(uuid.get_version_num() == 7),
    // Null returns true to allow nullable columns.
    Ok(None) => Ok(true),
    Err(_) => Ok(false),
  };
}

/// Creates a new UUIDv4 blob.
fn uuid_v4(_context: &Context) -> Result<[u8; 16], Error> {
  return Ok(Uuid::new_v4().into_bytes());
}

/// Creates a new UUIDv7 blob.
fn uuid_v7(_context: &Context) -> Result<[u8; 16], Error> {
  return Ok(Uuid::now_v7().into_bytes());
}

/// Format UUID blob as string-encoded UUID.
fn uuid_text(context: &Context) -> Result<String, Error> {
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return Ok(
    Uuid::from_slice(context.get_raw(0).as_blob()?)
      .map_err(|err| Error::UserFunctionError(err.into()))?
      .to_string(),
  );
}

/// Parse UUID from string-encoded UUID.
fn uuid_parse(context: &Context) -> Result<[u8; 16], Error> {
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return Ok(
    Uuid::try_parse(context.get_raw(0).as_str()?)
      .map_err(|err| Error::UserFunctionError(err.into()))?
      .into_bytes(),
  );
}

#[inline]
fn unpack_uuid_or_null(context: &Context<'_>) -> Result<Option<Uuid>, Error> {
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return match context.get_raw(0) {
    ValueRef::Null => Ok(None),
    ValueRef::Text(text) => Ok(Some(
      Uuid::try_parse(std::str::from_utf8(text)?)
        .map_err(|err| Error::UserFunctionError(err.into()))?,
    )),
    ValueRef::Blob(blob) => Ok(Some(
      Uuid::from_slice(blob).map_err(|err| Error::UserFunctionError(err.into()))?,
    )),
    _ => Err(FromSqlError::InvalidType.into()),
  };
}

pub(crate) fn register_extension_functions(
  db: &rusqlite::Connection,
) -> Result<(), rusqlite::Error> {
  // WARN: Be careful with declaring INNOCUOUS. It allows "user-defined functions" to run
  // when "trusted_schema=OFF", which means as part of: VIEWs, TRIGGERs, CHECK, DEFAULT,
  // GENERATED cols, ... as opposed to just top-level SELECTs.

  db.create_scalar_function(
    "is_uuid",
    1,
    FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_INNOCUOUS,
    is_uuid,
  )?;
  db.create_scalar_function(
    "is_uuid_v4",
    1,
    FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_INNOCUOUS,
    is_uuid_v4,
  )?;
  db.create_scalar_function("uuid_v4", 0, FunctionFlags::SQLITE_INNOCUOUS, uuid_v4)?;
  db.create_scalar_function(
    "is_uuid_v7",
    1,
    FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_INNOCUOUS,
    is_uuid_v7,
  )?;
  db.create_scalar_function("uuid_v7", 0, FunctionFlags::SQLITE_INNOCUOUS, uuid_v7)?;
  db.create_scalar_function(
    "uuid_text",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    uuid_text,
  )?;

  db.create_scalar_function(
    "uuid_parse",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    uuid_parse,
  )?;

  return Ok(());
}

#[cfg(test)]
mod tests {
  use rusqlite::{Error, params};
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
        .query_row("SELECT uuid_parse(uuid_text(uuid_v7()))", [], |row| {
          row.get::<_, [u8; 16]>(0)
        })
        .unwrap();

      assert_eq!(Uuid::from_slice(&row).unwrap().get_version_num(), 7);
    }

    assert!(
      conn
        .query_row("SELECT is_uuid_v7(uuid_v7())", [], |row| {
          row.get::<_, bool>(0)
        },)
        .unwrap()
    );
    assert!(
      conn
        .query_row("SELECT is_uuid_v4(uuid_v4())", [], |row| {
          row.get::<_, bool>(0)
        },)
        .unwrap()
    );
  }
}
