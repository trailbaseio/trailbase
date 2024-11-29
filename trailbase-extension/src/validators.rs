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
  use rusqlite::params;

  #[test]
  fn test_is_email() {
    let conn = crate::connect().unwrap();
    let create_table = r#"
        CREATE TABLE test (
          email                  TEXT CHECK(is_email(email))
        ) STRICT;
      "#;
    conn.execute(create_table, ()).unwrap();

    const QUERY: &str = "INSERT INTO test (email) VALUES ($1) RETURNING *";
    assert_eq!(
      conn
        .query_row(QUERY, ["test@test.com"], |row| Ok(row.get::<_, String>(0)?))
        .unwrap(),
      "test@test.com"
    );

    conn
      .query_row(QUERY, [rusqlite::types::Value::Null], |_row| Ok(()))
      .unwrap();

    assert!(conn.execute(QUERY, params!("not an email")).is_err());
  }

  #[test]
  fn test_is_json() {
    let conn = crate::connect().unwrap();
    let create_table = r#"
        CREATE TABLE test (
          json                   TEXT CHECK(is_json(json))
        ) STRICT;
      "#;
    conn.execute(create_table, ()).unwrap();

    const QUERY: &str = "INSERT INTO test (json) VALUES ($1)";
    conn.execute(QUERY, ["{}"]).unwrap();
    conn
      .execute(QUERY, ["{\"foo\": 42, \"bar\": {}, \"baz\": []}"])
      .unwrap();
    assert!(conn.execute(QUERY, [""]).is_err());
  }

  #[test]
  fn test_regexp() {
    let conn = crate::connect().unwrap();
    let create_table = "CREATE TABLE test (text0  TEXT, text1  TEXT) STRICT";
    conn.execute(create_table, ()).unwrap();

    const QUERY: &str = "INSERT INTO test (text0, text1) VALUES ($1, $2)";
    conn.execute(QUERY, ["abc123", "abc"]).unwrap();
    conn.execute(QUERY, ["def123", "def"]).unwrap();

    {
      let mut stmt = conn
        .prepare("SELECT * FROM test WHERE text1 REGEXP '^abc$'")
        .unwrap();
      let mut rows = stmt.query(()).unwrap();
      let mut cnt = 0;
      while let Some(row) = rows.next().unwrap() {
        assert_eq!("abc123", row.get::<_, String>(0).unwrap());
        cnt += 1;
      }
      assert_eq!(cnt, 1);
    }

    {
      let mut stmt = conn
        .prepare("SELECT * FROM test WHERE text1 REGEXP $1")
        .unwrap();
      let mut rows = stmt.query(params!(".*bc$")).unwrap();
      let mut cnt = 0;
      while let Some(row) = rows.next().unwrap() {
        assert_eq!("abc123", row.get::<_, String>(0).unwrap());
        cnt += 1;
      }
      assert_eq!(cnt, 1);
    }

    {
      let mut stmt = conn
        .prepare(r#"SELECT * FROM test WHERE text0 REGEXP '12\d'"#)
        .unwrap();
      let mut rows = stmt.query(()).unwrap();
      let mut cnt = 0;
      while let Some(_row) = rows.next().unwrap() {
        cnt += 1;
      }
      assert_eq!(cnt, 2);
    }
  }
}
