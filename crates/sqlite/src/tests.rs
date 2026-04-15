use futures_util::future::join_all;
use rusqlite::hooks::PreUpdateCase;
use rusqlite::{ErrorCode, ffi};
use serde::Deserialize;
use std::borrow::Cow;

use crate::connection::{Connection, Options};
use crate::sqlite::extract_row_id;
use crate::{Database, Error, Value, ValueType};

#[tokio::test]
async fn open_in_memory_test() {
  let conn = Connection::open_in_memory().unwrap();
  assert_eq!(1, conn.threads());
  assert!(conn.close().await.is_ok());
}

#[tokio::test]
async fn call_success_test() {
  let conn = Connection::open_in_memory().unwrap();

  let result = conn
    .call_writer(|conn| {
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
    .call_writer(|conn| conn.execute("Invalid sql", []).map_err(|e| e.into()))
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
  let db_path = tmp_dir.path().join("main.sqlite");

  let conn = Connection::with_opts(
    move || rusqlite::Connection::open(&db_path),
    Options {
      num_threads: Some(3),
      ..Default::default()
    },
  )
  .unwrap();

  assert_eq!(3, conn.threads());

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
    .call_writer(|conn| conn.execute("SELECT 1;", []).map_err(|e| e.into()))
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
    .call_writer(|conn| {
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
    .call_writer(|conn| {
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
      crate::Error::Rusqlite(e) => {
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
    .call_writer(|conn| failable_func(conn).map_err(|e| Error::Other(Box::new(e))))
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
    .call_writer(|conn| {
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

  let rows = crate::sqlite::execute_batch(
    &conn,
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

  // Lastly make sure rusqlite and out Connection consistently execute batches
  // containing statements returning rows.
  let query = "
    SELECT * FROM foo;
    SELECT * FROM foo;
  ";
  conn.write_lock().execute_batch(query).unwrap();
  conn.execute_batch(query).await.unwrap();
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
      .write_query_row_get(format!("SELECT COUNT(*) FROM {table}"), (), 0)
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

#[tokio::test]
async fn test_identity() {
  let conn0 = Connection::open_in_memory().unwrap();
  let conn1 = Connection::open_in_memory().unwrap();

  assert_ne!(conn0, conn1);
  assert_eq!(conn0, conn0.clone());
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
    .call_writer(|conn| {
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
    .read_query_row_get("SELECT COUNT(*) FROM person", (), 0)
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

  let (sender, receiver) = flume::unbounded::<String>();

  conn
    .write_lock()
    .preupdate_hook({
      let conn = conn.clone();
      Some(
        move |action: rusqlite::hooks::Action,
              _db: &str,
              table_name: &str,
              case: &PreUpdateCase| {
          let row_id = extract_row_id(case).unwrap();
          let state = State {
            action,
            table_name: table_name.to_string(),
            row_id,
          };

          if state.action != rusqlite::hooks::Action::SQLITE_INSERT {
            panic!("unexpected action: {:?}", state.action);
          }

          let sender = sender.clone();
          let conn = conn.clone();

          // We can't lock here since the lock is held by the `execute` below triggering
          // the hook. Thus delay the query until after the `execute` completes.
          std::thread::spawn(move || {
            let query = format!("SELECT text FROM '{}' WHERE _rowid_ = $1", state.table_name);
            sender
              .send(
                conn
                  .write_lock()
                  .query_row(&query, (state.row_id,), |row| row.get(0))
                  .unwrap(),
              )
              .unwrap();
          });
        },
      )
    })
    .unwrap();

  conn
    .execute("INSERT INTO test (id, text) VALUES (5, 'foo')", ())
    .await
    .unwrap();

  let text = receiver.recv().unwrap();
  assert_eq!(text, "foo");
}

// The rest is boilerplate, not really that important
#[derive(Debug, thiserror::Error)]
enum MyError {
  #[error("MySpecificError")]
  MySpecificError,
}

#[tokio::test]
async fn test_attach() {
  let conn = Connection::open_in_memory().unwrap();

  assert!(
    conn
      .execute(
        "
          CREATE TABLE test (id INTEGER PRIMARY KEY);
          ATTACH DATABASE ':memory:' AS foo;
        ",
        (),
      )
      .await
      .is_err()
  );

  conn
    .write_query_rows("ATTACH DATABASE ':memory:' AS foo", ())
    .await
    .unwrap();

  conn.attach(":memory:", "bar").unwrap();

  let databases = conn.list_databases().await.unwrap();
  assert_eq!(databases.len(), 3);
  assert_eq!(
    databases[2],
    Database {
      seq: 3,
      name: "bar".to_string(),
    }
  );

  conn.detach("bar").unwrap();
  conn.detach("foo").unwrap();

  let databases = conn.list_databases().await.unwrap();
  assert_eq!(databases.len(), 1);
}

#[test]
fn test_busy() {
  let rt = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(4)
    .enable_all()
    .build()
    .unwrap();

  let tmp_dir = tempfile::TempDir::new().unwrap();
  let db_path = tmp_dir.path().join("main.sqlite");

  let conn = Connection::with_opts(
    move || -> Result<_, rusqlite::Error> {
      let conn = rusqlite::Connection::open(&db_path)?;

      // NOTE: The busy handler should NEVER be triggered, since exclusive locking happens on the
      // `Connection` level. BUSY_TIMEOUT would only trigger if multiple SQLite connections
      // were trying to lock the DB concurrently.
      conn
        .busy_timeout(std::time::Duration::from_micros(1))
        .unwrap();
      conn
        .busy_handler(Some(|_| -> bool {
          assert!(false);
          return false;
        }))
        .unwrap();

      return Ok(conn);
    },
    Options {
      num_threads: Some(3),
      ..Default::default()
    },
  )
  .unwrap();

  rt.block_on(async move {
    conn
      .execute_batch(
        "
        CREATE TABLE test (
            id    INTEGER PRIMARY KEY,
            data  TEXT
        ) STRICT;

        INSERT INTO test (data) VALUES ('a'), ('b'), ('c');
      ",
      )
      .await
      .unwrap();

    // We use writes for SELECTs to trigger busy states.
    const N: usize = 10000;
    let futures: Vec<_> = (0..N)
      .map(|_| {
        let conn = conn.clone();
        return tokio::spawn(async move {
          conn
            .write_query_rows("SELECT * FROM test", ())
            .await
            .unwrap();
        });
      })
      .collect();

    for result in join_all(futures).await {
      result.unwrap();
    }
  });
}
