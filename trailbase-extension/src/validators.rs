use lru::LruCache;
use parking_lot::Mutex;
use regex::Regex;
use sqlite_loadable::prelude::*;
use sqlite_loadable::{api, Error, ErrorKind};
use std::num::NonZeroUsize;
use std::sync::LazyLock;
use validator::ValidateEmail;

/// Custom regexp function.
///
/// NOTE: Sqlite supports `col REGEXP pattern` in expression, which requires a custom
/// `regexp(pattern, col)` scalar function to be registered.
pub(super) fn regexp(
  context: *mut sqlite3_context,
  values: &[*mut sqlite3_value],
) -> Result<(), Error> {
  type CacheType = LazyLock<Mutex<LruCache<String, Regex>>>;
  static REGEX_CACHE: CacheType =
    LazyLock::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(128).unwrap())));

  if values.len() != 2 {
    return Err(Error::new_message("Expected 2 arguments"));
  }

  let string = &values[1];
  let valid = match api::value_type(string) {
    api::ValueType::Null => true,
    api::ValueType::Text => {
      let contents = api::value_text(string)?;
      let re = api::value_text(&values[0])?;

      let pattern: Option<Regex> = REGEX_CACHE.lock().get(re).cloned();
      match pattern {
        Some(pattern) => pattern.is_match(contents),
        None => {
          let pattern = Regex::new(re)
            .map_err(|err| Error::new(ErrorKind::Message(format!("Regex: {err}"))))?;

          let valid = pattern.is_match(contents);
          REGEX_CACHE.lock().push(re.to_string(), pattern);
          valid
        }
      }
    }
    _ => false,
  };

  api::result_bool(context, valid);

  Ok(())
}

pub(super) fn is_email(
  context: *mut sqlite3_context,
  values: &[*mut sqlite3_value],
) -> Result<(), Error> {
  if values.len() != 1 {
    return Err(Error::new_message("Expected 1 argument"));
  }

  let value = &values[0];
  let valid = match api::value_type(value) {
    api::ValueType::Null => true,
    api::ValueType::Text => {
      let contents = api::value_text(value)?;
      contents.validate_email()
    }
    _ => false,
  };

  api::result_bool(context, valid);

  Ok(())
}

pub(super) fn is_json(
  context: *mut sqlite3_context,
  values: &[*mut sqlite3_value],
) -> Result<(), Error> {
  if values.len() != 1 {
    return Err(Error::new_message("Expected 1 argument"));
  }

  let value = &values[0];
  let valid = match api::value_type(value) {
    api::ValueType::Null => true,
    api::ValueType::Text => {
      let contents = api::value_text(value)?;
      serde_json::from_str::<serde_json::Value>(contents)
        .map_err(|err| Error::new(ErrorKind::Message(format!("JSON: {err}"))))?;
      true
    }
    _ => false,
  };

  api::result_bool(context, valid);

  Ok(())
}

#[cfg(test)]
mod tests {
  use libsql::{params, Connection};

  async fn query_row(
    conn: &Connection,
    sql: &str,
    params: impl libsql::params::IntoParams,
  ) -> Result<libsql::Row, libsql::Error> {
    conn.prepare(sql).await?.query_row(params).await
  }

  #[tokio::test]
  async fn test_is_email() {
    let conn = crate::connect().await.unwrap();
    let create_table = r#"
        CREATE TABLE test (
          email                  TEXT CHECK(is_email(email))
        ) STRICT;
      "#;
    conn.query(create_table, ()).await.unwrap();

    const QUERY: &str = "INSERT INTO test (email) VALUES ($1) RETURNING *";
    assert_eq!(
      query_row(&conn, QUERY, ["test@test.com"])
        .await
        .unwrap()
        .get::<String>(0)
        .unwrap(),
      "test@test.com"
    );

    query_row(&conn, QUERY, [libsql::Value::Null])
      .await
      .unwrap();

    assert!(conn.execute(QUERY, params!("not an email")).await.is_err());
  }

  #[tokio::test]
  async fn test_is_json() {
    let conn = crate::connect().await.unwrap();
    let create_table = r#"
        CREATE TABLE test (
          json                   TEXT CHECK(is_json(json))
        ) STRICT;
      "#;
    conn.query(create_table, ()).await.unwrap();

    const QUERY: &str = "INSERT INTO test (json) VALUES ($1)";
    conn.execute(QUERY, ["{}"]).await.unwrap();
    conn
      .execute(QUERY, ["{\"foo\": 42, \"bar\": {}, \"baz\": []}"])
      .await
      .unwrap();
    assert!(conn.execute(QUERY, [""]).await.is_err());
  }

  #[tokio::test]
  async fn test_regexp() {
    let conn = crate::connect().await.unwrap();
    let create_table = "CREATE TABLE test (text0  TEXT, text1  TEXT) STRICT";
    conn.query(create_table, ()).await.unwrap();

    const QUERY: &str = "INSERT INTO test (text0, text1) VALUES ($1, $2)";
    conn.execute(QUERY, ["abc123", "abc"]).await.unwrap();
    conn.execute(QUERY, ["def123", "def"]).await.unwrap();

    {
      let mut rows = conn
        .query("SELECT * FROM test WHERE text1 REGEXP '^abc$'", ())
        .await
        .unwrap();
      let mut cnt = 0;
      while let Some(row) = rows.next().await.unwrap() {
        assert_eq!("abc123", row.get::<String>(0).unwrap());
        cnt += 1;
      }
      assert_eq!(cnt, 1);
    }

    {
      let mut rows = conn
        .query("SELECT * FROM test WHERE text1 REGEXP $1", params!(".*bc$"))
        .await
        .unwrap();
      let mut cnt = 0;
      while let Some(row) = rows.next().await.unwrap() {
        assert_eq!("abc123", row.get::<String>(0).unwrap());
        cnt += 1;
      }
      assert_eq!(cnt, 1);
    }

    {
      let mut rows = conn
        .query(r#"SELECT * FROM test WHERE text0 REGEXP '12\d'"#, ())
        .await
        .unwrap();
      let mut cnt = 0;
      while let Some(_row) = rows.next().await.unwrap() {
        cnt += 1;
      }
      assert_eq!(cnt, 2);
    }
  }
}
