use lru::LruCache;
use parking_lot::Mutex;
use regex::Regex;
use rusqlite::functions::Context;
use rusqlite::types::ValueRef;
use rusqlite::Error;
use std::num::NonZeroUsize;
use std::sync::LazyLock;
use validator::ValidateEmail;

/// Custom regexp function.
///
/// NOTE: Sqlite supports `col REGEXP pattern` in expression, which requires a custom
/// `regexp(pattern, col)` scalar function to be registered.
pub(super) fn regexp(context: &Context) -> rusqlite::Result<bool> {
  type CacheType = LazyLock<Mutex<LruCache<String, Regex>>>;
  static REGEX_CACHE: CacheType =
    LazyLock::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(128).expect("infallible"))));

  #[cfg(debug_assertions)]
  if context.len() != 2 {
    return Err(Error::InvalidParameterCount(context.len(), 2));
  }

  // let string = &values[1];
  return match context.get_raw(1) {
    ValueRef::Null => Ok(true),
    ValueRef::Text(ascii) => {
      let contents = String::from_utf8_lossy(ascii);
      let re = context.get_raw(0).as_str()?;

      let pattern: Option<Regex> = REGEX_CACHE.lock().get(re).cloned();
      match pattern {
        Some(pattern) => Ok(pattern.is_match(&contents)),
        None => {
          let pattern = Regex::new(re)
            .map_err(|err| Error::UserFunctionError(format!("Regex: {err}").into()))?;

          let valid = pattern.is_match(&contents);
          REGEX_CACHE.lock().push(re.to_string(), pattern);
          Ok(valid)
        }
      }
    }
    _ => Ok(false),
  };
}

pub(super) fn is_email(context: &Context) -> rusqlite::Result<bool> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return match context.get_raw(0).as_str_or_null()? {
    None => Ok(true),
    Some(str) => Ok(str.validate_email()),
  };
}

pub(super) fn is_json(context: &Context) -> rusqlite::Result<bool> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return match context.get_raw(0).as_str_or_null()? {
    None => Ok(true),
    Some(str) => {
      if serde_json::from_str::<serde_json::Value>(str).is_ok() {
        Ok(true)
      } else {
        Ok(false)
      }
    }
  };
}

#[cfg(test)]
mod tests {
  use rusqlite::params;

  #[test]
  fn test_is_email() {
    let conn = crate::connect_sqlite(None, None).unwrap();
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
    let conn = crate::connect_sqlite(None, None).unwrap();
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
    let conn = crate::connect_sqlite(None, None).unwrap();
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
