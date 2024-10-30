use base64::prelude::*;
use sqlite_loadable::prelude::*;
use sqlite_loadable::{api, Error, ErrorKind, Result};
use uuid::Uuid;

pub(super) fn is_uuid(context: *mut sqlite3_context, values: &[*mut sqlite3_value]) {
  match unpack_uuid_or_null(values) {
    Ok(Some(uuid)) => api::result_bool(context, uuid.get_version_num() == 7),
    Ok(None) => api::result_bool(context, true),
    _ => api::result_bool(context, false),
  };
}

pub(super) fn is_uuid_v7(context: *mut sqlite3_context, values: &[*mut sqlite3_value]) {
  match unpack_uuid_or_null(values) {
    Ok(Some(uuid)) => api::result_bool(context, uuid.get_version_num() == 7),
    Ok(None) => api::result_bool(context, true),
    _ => api::result_bool(context, false),
  };
}

pub(super) fn uuid_url_safe_b64(
  context: *mut sqlite3_context,
  values: &[*mut sqlite3_value],
) -> Result<()> {
  if let Some(uuid) = unpack_uuid_or_null(values)? {
    let _ = api::result_text(context, BASE64_URL_SAFE.encode(uuid.as_bytes()));
  }

  return Ok(());
}

#[inline(always)]
fn unpack_uuid_or_null(values: &[*mut sqlite3_value]) -> Result<Option<Uuid>> {
  if values.len() != 1 {
    return Err(Error::new_message("Wrong number of arguments"));
  }

  let value = &values[0];
  return match api::value_type(value) {
    api::ValueType::Null => Ok(None),
    api::ValueType::Blob => match Uuid::from_slice(api::value_blob(value)) {
      Ok(uuid) => Ok(Some(uuid)),
      Err(err) => Err(Error::new(ErrorKind::Message(format!(
        "Failed to read uuid: {err}"
      )))),
    },
    _ => Err(Error::new_message("Expected BLOB column type.")),
  };
}

pub(super) fn uuid_v7_text(
  context: *mut sqlite3_context,
  _values: &[*mut sqlite3_value],
) -> Result<()> {
  api::result_text(context, Uuid::now_v7().to_string())
}

pub(super) fn uuid_v7(context: *mut sqlite3_context, _values: &[*mut sqlite3_value]) {
  api::result_blob(context, Uuid::now_v7().as_bytes());
}

pub(super) fn parse_uuid(
  context: *mut sqlite3_context,
  values: &[*mut sqlite3_value],
) -> Result<()> {
  if values.len() != 1 {
    return Err(Error::new_message("Wrong number of arguments"));
  }
  let value: &str = api::value_text(&values[0])?;
  let id = Uuid::parse_str(value)
    .map_err(|err| Error::new(ErrorKind::Message(format!("UUID parse: {err}"))))?;

  api::result_blob(context, id.as_bytes());

  Ok(())
}

#[cfg(test)]
mod tests {
  use libsql::{params, Connection};
  use uuid::Uuid;

  async fn query_row(
    conn: &Connection,
    sql: &str,
    params: impl libsql::params::IntoParams,
  ) -> Result<libsql::Row, libsql::Error> {
    conn.prepare(sql).await?.query_row(params).await
  }

  #[tokio::test]
  async fn test_uuid() {
    let conn = crate::connect().await.unwrap();

    let create_table = r#"
        CREATE TABLE test (
          id                           BLOB PRIMARY KEY NOT NULL DEFAULT (uuid_v7()),
          uuid                         BLOB CHECK(is_uuid(uuid)),
          uuid_v7                      BLOB CHECK(is_uuid_v7(uuid_v7))
        ) STRICT;
      "#;
    conn.query(create_table, ()).await.unwrap();

    {
      let row = query_row(
        &conn,
        "INSERT INTO test (uuid, uuid_v7) VALUES (NULL, NULL) RETURNING id",
        (),
      )
      .await
      .unwrap();

      Uuid::from_slice(&row.get::<[u8; 16]>(0).unwrap()).unwrap();
    }

    {
      assert!(conn
        .execute(
          "INSERT INTO test (uuid, uuid_v7) VALUES ($1, NULL)",
          params!(b"")
        )
        .await
        .is_err());
    }

    {
      let uuidv4 = Uuid::new_v4();
      assert!(conn
        .execute(
          "INSERT INTO test (uuid, uuid_v7) VALUES (NULL, $1)",
          params!(uuidv4.into_bytes().to_vec())
        )
        .await
        .is_err());
    }

    {
      let uuid = Uuid::now_v7();
      let row = query_row(
        &conn,
        "INSERT INTO test (uuid, uuid_v7) VALUES (parse_uuid($1), parse_uuid($1)) RETURNING uuid",
        [uuid.to_string()],
      )
      .await
      .unwrap();

      assert_eq!(
        Uuid::from_slice(&row.get::<[u8; 16]>(0).unwrap()).unwrap(),
        uuid
      );
    }
  }
}
