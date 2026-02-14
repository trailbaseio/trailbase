use rusqlite::Error;
use rusqlite::functions::{Context, FunctionFlags};
use validator::ValidateEmail;

fn is_email(context: &Context) -> Result<bool, Error> {
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  if let Some(str) = context.get_raw(0).as_str_or_null()? {
    return Ok(str.validate_email());
  }
  return Ok(true);
}

fn is_json(context: &Context) -> Result<bool, Error> {
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  if let Some(str) = context.get_raw(0).as_str_or_null()? {
    return Ok(serde_json::from_str::<serde_json::Value>(str).is_ok());
  }
  return Ok(true);
}

pub(crate) fn register_extension_functions(db: &rusqlite::Connection) -> Result<(), Error> {
  // WARN: Be careful with declaring INNOCUOUS. It allows "user-defined functions" to run
  // when "trusted_schema=OFF", which means as part of: VIEWs, TRIGGERs, CHECK, DEFAULT,
  // GENERATED cols, ... as opposed to just top-level SELECTs.

  db.create_scalar_function(
    "is_email",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    is_email,
  )?;
  // NOTE: there's also https://sqlite.org/json1.html#jvalid
  db.create_scalar_function(
    "is_json",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    is_json,
  )?;

  return Ok(());
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
