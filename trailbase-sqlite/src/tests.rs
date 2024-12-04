use rusqlite::ffi;
use serde::Deserialize;

use crate::{named_params, params, Connection, Error, Value, ValueType};
use rusqlite::ErrorCode;

#[test]
fn call_success_test() {
  let conn = Connection::open_in_memory();

  let result = conn.call(|conn| {
    conn
      .execute(
        "CREATE TABLE person(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);",
        [],
      )
      .map_err(|e| e.into())
  });

  assert_eq!(0, result.unwrap());
}

#[test]
fn call_failure_test() {
  let conn = Connection::open_in_memory();

  let result = conn.call(|conn| conn.execute("Invalid sql", []).map_err(|e| e.into()));

  assert!(match result.unwrap_err() {
    crate::Error::Rusqlite(e) => {
      e == rusqlite::Error::SqlInputError {
        error: ffi::Error {
          code: ErrorCode::Unknown,
          extended_code: 1,
        },
        msg: "near \"Invalid\": syntax error".to_string(),
        sql: "Invalid sql".to_string(),
        offset: 0,
      }
    }
    _ => false,
  });
}

#[test]
fn debug_format_test() {
  let conn = Connection::open_in_memory();

  assert_eq!("Connection".to_string(), format!("{conn:?}"));
}

fn failable_func(_: &rusqlite::Connection) -> std::result::Result<(), MyError> {
  Err(MyError::MySpecificError)
}

#[test]
fn test_ergonomic_errors() {
  let conn = Connection::open_in_memory();

  let res = conn
    .call(|conn| failable_func(conn).map_err(|e| Error::Other(Box::new(e))))
    .unwrap_err();

  let err = std::error::Error::source(&res)
    .and_then(|e| e.downcast_ref::<MyError>())
    .unwrap();

  assert!(matches!(err, MyError::MySpecificError), "Got: {err:?}");
}

#[test]
fn test_construction() {
  let _conn = Connection::from_conn(|| rusqlite::Connection::open_in_memory().unwrap());
}

#[test]
fn test_query() {
  let conn = Connection::open_in_memory();

  let result = conn.call(|conn| {
    conn
      .execute(
        "CREATE TABLE person(id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
        [],
      )
      .map_err(|e| e.into())
  });

  assert_eq!(0, result.unwrap());

  conn
    .query(
      "INSERT INTO person (id, name) VALUES ($1, $2)",
      params!(0, "foo"),
    )
    .unwrap();
  conn
    .query(
      "INSERT INTO person (id, name) VALUES (:id, :name)",
      named_params! {":id": 1, ":name": "bar"},
    )
    .unwrap();

  let rows = conn.query("SELECT * FROM person", ()).unwrap();
  assert_eq!(2, rows.len());
  assert!(matches!(rows.column_type(0).unwrap(), ValueType::Integer));
  assert_eq!(rows.column_name(0).unwrap(), "id");

  assert!(matches!(rows.column_type(1).unwrap(), ValueType::Text));
  assert_eq!(rows.column_name(1).unwrap(), "name");

  conn
    .execute("UPDATE person SET name = 'baz' WHERE id = $1", (1,))
    .unwrap();

  let row = conn
    .query_row("SELECT name FROM person WHERE id = $1", &[1])
    .unwrap()
    .unwrap();

  assert_eq!(row.get::<String>(0).unwrap(), "baz");

  #[derive(Deserialize)]
  struct Person {
    id: i64,
    name: String,
  }

  let person = conn
    .query_value::<Person>("SELECT * FROM person WHERE id = $1", &[1])
    .unwrap()
    .unwrap();
  assert_eq!(person.id, 1);
  assert_eq!(person.name, "baz");

  let rows = conn
    .execute_batch(
      r#"
            CREATE TABLE foo (id INTEGER) STRICT;
            INSERT INTO foo (id) VALUES (17);
            SELECT * FROM foo;
        "#,
    )
    .unwrap()
    .unwrap();
  assert_eq!(rows.len(), 1);
  assert_eq!(rows.0.get(0).unwrap().get::<i64>(0), Ok(17));
}

#[test]
fn test_params() {
  let _ = named_params! {
      ":null": None::<String>,
      ":text": Some("test".to_string()),
  };

  let conn = Connection::open_in_memory();

  conn
    .call(|conn| {
      conn
        .execute(
          "CREATE TABLE person(id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
          [],
        )
        .map_err(|e| e.into())
    })
    .unwrap();

  conn
    .query(
      "INSERT INTO person (id, name) VALUES (:id, :name)",
      [
        (":id", Value::Integer(1)),
        (":name", Value::Text("Alice".to_string())),
      ],
    )
    .unwrap();

  let id = 3;
  conn
    .query(
      "INSERT INTO person (id, name) VALUES (:id, :name)",
      named_params! {
          ":id": id,
          ":name": Value::Text("Eve".to_string()),
      },
    )
    .unwrap();

  conn
    .query(
      "INSERT INTO person (id, name) VALUES ($1, $2)",
      [Value::Integer(2), Value::Text("Bob".to_string())],
    )
    .unwrap();

  conn
    .query(
      "INSERT INTO person (id, name) VALUES ($1, $2)",
      params!(4, "Jay"),
    )
    .unwrap();

  let rows = conn.query("SELECT COUNT(*) FROM person", ()).unwrap();

  assert_eq!(rows.0.get(0).unwrap().get::<i64>(0), Ok(4));
}

// The rest is boilerplate, not really that important

#[derive(Debug, thiserror::Error)]
enum MyError {
  #[error("MySpecificError")]
  MySpecificError,
}
