use rusqlite::ffi;
use rusqlite::hooks::PreUpdateCase;
use serde::Deserialize;
use std::borrow::Cow;

use crate::connection::{Connection, Error, Options, extract_row_id};
use crate::{Value, ValueType, named_params, params};
use rusqlite::ErrorCode;

#[tokio::test]
async fn open_in_memory_test() {
  let conn = Connection::open_in_memory().unwrap();
  assert!(conn.close().await.is_ok());
}

#[tokio::test]
async fn call_success_test() {
  let conn = Connection::open_in_memory().unwrap();

  let result = conn
    .call(|conn| {
      conn
        .execute(
          "CREATE TABLE person(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);",
          [],
        )
        .map_err(|e| e.into())
    })
    .await;

  assert_eq!(0, result.unwrap());
}

#[tokio::test]
async fn call_failure_test() {
  let conn = Connection::open_in_memory().unwrap();

  let result = conn
    .call(|conn| conn.execute("Invalid sql", []).map_err(|e| e.into()))
    .await;

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

#[tokio::test]
async fn close_success_test() {
  let tmp_dir = tempfile::TempDir::new().unwrap();
  let fname = tmp_dir.path().join("main.sqlite");

  let conn = Connection::new(
    move || rusqlite::Connection::open(&fname),
    Some(Options {
      n_read_threads: 2,
      ..Default::default()
    }),
  )
  .unwrap();

  conn
    .execute("CREATE TABLE 'test' (id INTEGER PRIMARY KEY)", ())
    .await
    .unwrap();
  conn
    .execute_batch(
      r#"
        INSERT INTO 'test' (id) VALUES (1);
        INSERT INTO 'test' (id) VALUES (2);
      "#,
    )
    .await
    .unwrap();

  let rows = conn
    .read_query_rows("SELECT * FROM 'test'", ())
    .await
    .unwrap();

  assert_eq!(rows.len(), 2);

  assert!(conn.close().await.is_ok());
}

#[tokio::test]
async fn double_close_test() {
  let conn = Connection::open_in_memory().unwrap();

  let conn2 = conn.clone();

  assert!(conn.close().await.is_ok());
  assert!(conn2.close().await.is_ok());
}

#[tokio::test]
async fn close_call_test() {
  let conn = Connection::open_in_memory().unwrap();

  let conn2 = conn.clone();

  assert!(conn.close().await.is_ok());

  let result = conn2
    .call(|conn| conn.execute("SELECT 1;", []).map_err(|e| e.into()))
    .await;

  assert!(matches!(
    result.unwrap_err(),
    crate::Error::ConnectionClosed
  ));
}

#[tokio::test]
async fn close_failure_test() {
  let conn = Connection::open_in_memory().unwrap();

  conn
    .call(|conn| {
      conn
        .execute(
          "CREATE TABLE person(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);",
          [],
        )
        .map_err(|e| e.into())
    })
    .await
    .unwrap();

  conn
    .call(|conn| {
      // Leak a prepared statement to make the database uncloseable
      // See https://www.sqlite.org/c3ref/close.html for details regarding this behaviour
      let stmt = Box::new(conn.prepare("INSERT INTO person VALUES (1, ?1);").unwrap());
      Box::leak(stmt);
      Ok(())
    })
    .await
    .unwrap();

  let err = conn.close().await.unwrap_err();
  assert!(
    match &err {
      crate::Error::Close(e) => {
        *e == rusqlite::Error::SqliteFailure(
          ffi::Error {
            code: ErrorCode::DatabaseBusy,
            extended_code: 5,
          },
          Some("unable to close due to unfinalized statements or unfinished backups".to_string()),
        )
      }
      _ => false,
    },
    "Error: {err:?}"
  );
}

#[tokio::test]
async fn debug_format_test() {
  let conn = Connection::open_in_memory().unwrap();

  assert_eq!("Connection".to_string(), format!("{conn:?}"));
}

#[tokio::test]
async fn test_error_source() {
  let error = crate::Error::Rusqlite(rusqlite::Error::InvalidQuery);
  assert_eq!(
    std::error::Error::source(&error)
      .and_then(|e| e.downcast_ref::<rusqlite::Error>())
      .unwrap(),
    &rusqlite::Error::InvalidQuery,
  );
}

fn failable_func(_: &rusqlite::Connection) -> std::result::Result<(), MyError> {
  Err(MyError::MySpecificError)
}

#[tokio::test]
async fn test_ergonomic_errors() {
  let conn = Connection::open_in_memory().unwrap();

  let res = conn
    .call(|conn| failable_func(conn).map_err(|e| Error::Other(Box::new(e))))
    .await
    .unwrap_err();

  let err = std::error::Error::source(&res)
    .and_then(|e| e.downcast_ref::<MyError>())
    .unwrap();

  assert!(matches!(err, MyError::MySpecificError));
}

#[tokio::test]
async fn test_execute_and_query() {
  let conn = Connection::open_in_memory().unwrap();

  let result = conn
    .call(|conn| {
      conn
        .execute(
          "CREATE TABLE person(id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
          [],
        )
        .map_err(|e| e.into())
    })
    .await;

  assert_eq!(0, result.unwrap());

  conn
    .execute(
      "INSERT INTO person (id, name) VALUES ($1, $2)",
      params!(0, "foo"),
    )
    .await
    .unwrap();
  conn
    .execute(
      "INSERT INTO person (id, name) VALUES (:id, :name)",
      named_params! {":id": 1, ":name": "bar"},
    )
    .await
    .unwrap();

  let rows = conn
    .read_query_rows(Cow::Borrowed("SELECT * FROM person"), ())
    .await
    .unwrap();
  assert_eq!(2, rows.len());
  assert!(matches!(rows.column_type(0).unwrap(), ValueType::Integer));
  assert_eq!(rows.column_name(0).unwrap(), "id");

  assert!(matches!(rows.column_type(1).unwrap(), ValueType::Text));
  assert_eq!(rows.column_name(1).unwrap(), "name");

  conn
    .execute("UPDATE person SET name = 'baz' WHERE id = $1", (1,))
    .await
    .unwrap();

  let row = conn
    .read_query_row("SELECT name FROM person WHERE id = $1", &[1])
    .await
    .unwrap()
    .unwrap();

  assert_eq!(row.get::<String>(0).unwrap(), "baz");

  #[derive(Deserialize)]
  struct Person {
    id: i64,
    name: String,
  }

  let person = conn
    .read_query_value::<Person>("SELECT * FROM person WHERE id = $1", &[1])
    .await
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
    .await
    .unwrap()
    .unwrap();
  assert_eq!(rows.len(), 1);
  assert_eq!(rows.0.get(0).unwrap().get::<i64>(0), Ok(17));
}

#[tokio::test]
async fn test_execute_batch() {
  let conn = Connection::open_in_memory().unwrap();
  let _ = conn
    .execute_batch(
      r#"
        CREATE TABLE parent (
          id           INTEGER PRIMARY KEY NOT NULL,
          value        TEXT NOT NULL
        ) STRICT;

        INSERT INTO parent (id, value) VALUES (1, 'first'), (2, 'second');

        CREATE TABLE child (
          id           INTEGER PRIMARY KEY NOT NULL,
          parent       INTEGER REFERENCES parent NOT NULL
        ) STRICT;

        INSERT INTO child (id, parent) VALUES (1, 1), (2, 2);
      "#,
    )
    .await
    .unwrap();

  let count = async |table: &str| -> i64 {
    return conn
      .query_row_f(format!("SELECT COUNT(*) FROM {table}"), (), |row| {
        row.get(0)
      })
      .await
      .unwrap()
      .unwrap();
  };

  assert_eq!(2, count("parent").await);
  assert_eq!(2, count("child").await);
}

#[tokio::test]
async fn test_execute_batch_error() {
  let conn = Connection::open_in_memory().unwrap();

  let result = conn.execute_batch("SELECT a").await;
  assert!(result.is_err(), "{result:?}");

  let result = conn.execute_batch("SELECT a;").await;
  assert!(result.is_err(), "{result:?}");

  let result = conn
    .execute_batch(
      r#"
        CREATE TABLE parent (id INTEGER PRIMARY KEY NOT NULL) STRICT;
        NOT VALID SQL;
      "#,
    )
    .await;

  assert!(result.is_err(), "{result:?}");
}

#[test]
fn test_locking() {
  let conn = Connection::open_in_memory().unwrap();
  let mut lock = conn.write_lock();

  let tx = lock.transaction().unwrap();
  tx.execute_batch(
    r#"
      CREATE TABLE 'table' (id INTEGER PRIMARY KEY, name TEXT);
      INSERT INTO 'table' (id, name) VALUES (1, 'Alice'), (2, 'Bob');
    "#,
  )
  .unwrap();
  tx.commit().unwrap();

  let name: String = lock
    .query_row("SELECT name FROM 'table' WHERE id = ?1", [2], |row| {
      row.get(0)
    })
    .unwrap();
  assert_eq!(name, "Bob");
}

#[tokio::test]
async fn test_params() {
  let _ = named_params! {
      ":null": None::<String>,
      ":text": Some("test".to_string()),
  };

  let conn = Connection::open_in_memory().unwrap();

  conn
    .call(|conn| {
      conn
        .execute(
          "CREATE TABLE person(id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
          [],
        )
        .map_err(|e| e.into())
    })
    .await
    .unwrap();

  conn
    .execute(
      "INSERT INTO person (id, name) VALUES (:id, :name)",
      [
        (":id", Value::Integer(1)),
        (":name", Value::Text("Alice".to_string())),
      ],
    )
    .await
    .unwrap();

  let id = 3;
  conn
    .execute(
      "INSERT INTO person (id, name) VALUES (:id, :name)",
      named_params! {
          ":id": id,
          ":name": Value::Text("Eve".to_string()),
      },
    )
    .await
    .unwrap();

  conn
    .execute(
      "INSERT INTO person (id, name) VALUES ($1, $2)",
      [Value::Integer(2), Value::Text("Bob".to_string())],
    )
    .await
    .unwrap();

  conn
    .execute(
      "INSERT INTO person (id, name) VALUES ($1, $2)",
      params!(4, "Jay"),
    )
    .await
    .unwrap();

  let count: i64 = conn
    .read_query_row_f("SELECT COUNT(*) FROM person", (), |row| row.get(0))
    .await
    .unwrap()
    .unwrap();

  assert_eq!(4, count);
}

#[tokio::test]
async fn test_hooks() {
  let conn = Connection::open_in_memory().unwrap();

  conn
    .execute("CREATE TABLE test (id INTEGER PRIMARY KEY, text TEXT)", ())
    .await
    .unwrap();

  struct State {
    action: rusqlite::hooks::Action,
    table_name: String,
    row_id: i64,
  }

  let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
  let c = conn.clone();

  conn
    .add_preupdate_hook(Some(
      move |action: rusqlite::hooks::Action, _db: &str, table_name: &str, case: &PreUpdateCase| {
        let row_id = extract_row_id(case).unwrap();
        let state = State {
          action,
          table_name: table_name.to_string(),
          row_id,
        };

        let sender = sender.clone();
        c.call_and_forget(move |conn| {
          match state.action {
            rusqlite::hooks::Action::SQLITE_INSERT => {
              let text = conn
                .query_row(
                  &format!(
                    r#"SELECT text FROM "{}" WHERE _rowid_ = $1"#,
                    state.table_name
                  ),
                  [state.row_id],
                  |row| row.get::<_, String>(0),
                )
                .unwrap();

              sender.send(text).unwrap();
            }
            _ => {
              panic!("unexpected action: {:?}", state.action);
            }
          };
        });
      },
    ))
    .await
    .unwrap();

  conn
    .execute("INSERT INTO test (id, text) VALUES (5, 'foo')", ())
    .await
    .unwrap();

  let text = receiver.recv().await.unwrap();
  assert_eq!(text, "foo");
}

// The rest is boilerplate, not really that important
#[derive(Debug, thiserror::Error)]
enum MyError {
  #[error("MySpecificError")]
  MySpecificError,
}
