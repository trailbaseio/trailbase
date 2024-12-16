use rusqlite::functions::Context;
use rusqlite::types::ValueRef;
use rusqlite::Error;
use uuid::Uuid;

pub(super) fn is_uuid(context: &Context) -> rusqlite::Result<bool> {
  return Ok(unpack_uuid_or_null(context).is_ok());
}

pub(super) fn is_uuid_v7(context: &Context) -> rusqlite::Result<bool> {
  return Ok(match unpack_uuid_or_null(context)? {
    Some(uuid) => uuid.get_version_num() == 7,
    None => true,
  });
}

#[inline]
fn unpack_uuid_or_null(context: &Context<'_>) -> rusqlite::Result<Option<Uuid>> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return match context.get_raw(0) {
    ValueRef::Null => Ok(None),
    ValueRef::Blob(b) => match Uuid::from_slice(b) {
      Ok(uuid) => Ok(Some(uuid)),
      Err(err) => Err(Error::UserFunctionError(
        format!("Failed to read uuid: {err}").into(),
      )),
    },
    _ => Err(Error::UserFunctionError(
      "Expected BLOB column type.".into(),
    )),
  };
}

pub(super) fn uuid_v7_text(_context: &Context) -> rusqlite::Result<String> {
  return Ok(Uuid::now_v7().to_string());
}

pub(super) fn uuid_v7(_context: &Context) -> rusqlite::Result<Vec<u8>> {
  return Ok(Uuid::now_v7().as_bytes().to_vec());
}

pub(super) fn parse_uuid(context: &Context) -> rusqlite::Result<Vec<u8>> {
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

#[cfg(test)]
mod tests {
  use rusqlite::params;
  use uuid::Uuid;

  #[test]
  fn test_uuid() {
    let conn = crate::connect().unwrap();

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
      let uuidv4 = Uuid::new_v4();
      assert!(conn
        .execute(
          "INSERT INTO test (uuid, uuid_v7) VALUES (NULL, $1)",
          params!(uuidv4.into_bytes().to_vec())
        )
        .is_err());
    }

    {
      let uuid = Uuid::now_v7();
      let row = conn
        .query_row(
          "INSERT INTO test (uuid, uuid_v7) VALUES (parse_uuid($1), parse_uuid($1)) RETURNING uuid",
          [uuid.to_string()],
          |row| -> rusqlite::Result<[u8; 16]> { Ok(row.get(0)?) },
        )
        .unwrap();

      assert_eq!(Uuid::from_slice(&row).unwrap(), uuid);
    }
  }
}
