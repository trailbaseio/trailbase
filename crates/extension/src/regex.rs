use mini_moka::sync::Cache;
use regex::Regex;
use rusqlite::Error;
use rusqlite::functions::Context;
use std::sync::LazyLock;

// NOTE: Regexps are using Arcs internally and are cheap to clone.
static CACHE: LazyLock<Cache<String, Regex>> = LazyLock::new(|| Cache::new(256));

/// Custom regexp function.
///
/// NOTE: Sqlite supports `col REGEXP pattern` in expression, which requires a custom
/// `regexp(pattern, col)` scalar function to be registered.
pub(super) fn regexp(context: &Context) -> Result<bool, Error> {
  #[cfg(debug_assertions)]
  if context.len() != 2 {
    return Err(Error::InvalidParameterCount(context.len(), 2));
  }

  return regexp_impl(
    context.get_raw(0).as_str()?,
    context.get_raw(1).as_str_or_null()?,
  );
}

#[inline]
fn regexp_impl(re: &str, contents: Option<&str>) -> Result<bool, Error> {
  let Some(contents) = contents else {
    return Ok(true);
  };

  let re = re.to_string();

  return match CACHE.get(&re) {
    Some(pattern) => Ok(pattern.is_match(contents)),
    None => {
      let pattern =
        Regex::new(&re).map_err(|err| Error::UserFunctionError(format!("Regex: {err}").into()))?;

      let valid = pattern.is_match(contents);
      CACHE.insert(re.to_string(), pattern);

      Ok(valid)
    }
  };
}

#[cfg(test)]
mod tests {
  use rusqlite::params;

  use super::*;

  #[test]
  fn test_regexp_impl() {
    assert!(regexp_impl("pattern", None).unwrap());
    assert!(!regexp_impl("pattern", Some("")).unwrap());
    assert!(regexp_impl(".*", Some("Something")).unwrap());

    assert!(CACHE.contains_key(&"pattern".to_string()), "{CACHE:?}");
    assert!(CACHE.contains_key(&".*".to_string()), "{CACHE:?}");
  }

  #[test]
  fn test_regexp() {
    let conn = crate::connect_sqlite(None).unwrap();
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
