use rusqlite::Error;
use rusqlite::functions::Context;
use validator::ValidateEmail;

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
}
