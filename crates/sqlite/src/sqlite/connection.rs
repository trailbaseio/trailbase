use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::database::Database;
use crate::error::Error;
use crate::from_sql::FromSql;
use crate::params::Params;
use crate::rows::{Row, Rows};
use crate::sqlite::executor::Executor;
use crate::sqlite::util::{columns, from_row, from_rows, get_value, map_first};

// NOTE: We should probably decouple from the impl.
pub use crate::sqlite::executor::{ArcLockGuard, LockGuard, Options};

/// A handle to call functions in background thread.
#[derive(Clone)]
pub struct Connection {
  id: usize,
  exec: Executor,
}

impl Connection {
  pub fn new<E>(builder: impl Fn() -> Result<rusqlite::Connection, E>) -> Result<Self, Error>
  where
    Error: From<E>,
  {
    return Self::with_opts(builder, Options::default());
  }

  pub fn with_opts<E>(
    builder: impl Fn() -> Result<rusqlite::Connection, E>,
    opt: Options,
  ) -> Result<Self, Error>
  where
    Error: From<E>,
  {
    return Ok(Self {
      id: UNIQUE_CONN_ID.fetch_add(1, Ordering::SeqCst),
      exec: Executor::new(builder, opt)?,
    });
  }

  /// Open a new connection to an in-memory SQLite database.
  ///
  /// # Failure
  ///
  /// Will return `Err` if the underlying SQLite open call fails.
  pub fn open_in_memory() -> Result<Self, Error> {
    let conn = Self::with_opts(
      rusqlite::Connection::open_in_memory,
      Options {
        num_threads: Some(1),
        ..Default::default()
      },
    )?;

    assert_eq!(1, conn.threads());

    return Ok(conn);
  }

  pub fn id(&self) -> usize {
    return self.id;
  }

  pub fn threads(&self) -> usize {
    return self.exec.threads();
  }

  pub fn write_lock(&self) -> LockGuard<'_> {
    return self.exec.write_lock();
  }

  pub fn try_write_arc_lock_for(&self, duration: tokio::time::Duration) -> Option<ArcLockGuard> {
    return self.exec.try_write_arc_lock_for(duration);
  }

  /// Call a function in background thread and get the result
  /// asynchronously.
  ///
  /// # Failure
  ///
  /// Will return `Err` if the database connection has been closed.
  ///
  /// # Notes
  ///
  /// This is a "leaky" API, leaking the internals of `rusqlite::Connection`. We cannot easily
  /// remove this API. Current use-cases include:
  ///
  /// * `conn.transaction()` for RecordApis & migrations (from admin via TransactionRecorder and
  ///   during startup/SIGHUP).
  /// * Batch log inserts to minimize thread slushing.
  /// * Backups from scheduler (API could be easily hoisted)
  pub async fn call_writer<F, R, E>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R, E> + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Error: From<E>,
  {
    return self.exec.call_writer(function).await;
  }

  pub async fn call_reader<F, R, E>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(&rusqlite::Connection) -> Result<R, E> + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Error: From<E>,
  {
    return self.exec.call_reader(function).await;
  }

  /// Query SQL statement.
  pub async fn read_query_rows(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Rows, Error> {
    return self.exec.read_query_rows_f(sql, params, from_rows).await;
  }

  pub async fn read_query_row(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<Row>, Error> {
    return self
      .exec
      .read_query_rows_f(sql, params, |rows| {
        return map_first(rows, |row| {
          return from_row(row, Arc::new(columns(row.as_ref())));
        });
      })
      .await;
  }

  pub async fn read_query_row_get<T>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    index: usize,
  ) -> Result<Option<T>, Error>
  where
    T: FromSql + Send + 'static,
  {
    return self
      .exec
      .read_query_rows_f(sql, params, move |rows| {
        return map_first(rows, move |row| {
          return get_value(row, index);
        });
      })
      .await;
  }

  pub async fn read_query_value<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<T>, Error> {
    return self
      .exec
      .read_query_rows_f(sql, params, |rows| {
        return map_first(rows, move |row| {
          serde_rusqlite::from_row(row).map_err(Error::DeserializeValue)
        });
      })
      .await;
  }

  pub async fn read_query_values<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Vec<T>, Error> {
    return self
      .exec
      .read_query_rows_f(sql, params, |rows| {
        return serde_rusqlite::from_rows(rows)
          .collect::<Result<Vec<_>, _>>()
          .map_err(Error::DeserializeValue);
      })
      .await;
  }

  pub async fn write_query_rows(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Rows, Error> {
    return self.exec.write_query_rows_f(sql, params, from_rows).await;
  }

  pub async fn write_query_row_get<T>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    index: usize,
  ) -> Result<Option<T>, Error>
  where
    T: FromSql + Send + 'static,
  {
    return self
      .exec
      .write_query_rows_f(sql, params, move |rows| {
        return map_first(rows, move |row| {
          return get_value(row, index);
        });
      })
      .await;
  }

  pub async fn write_query_value<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<T>, Error> {
    return self
      .exec
      .write_query_rows_f(sql, params, |rows| {
        return map_first(rows, |row| {
          serde_rusqlite::from_row(row).map_err(Error::DeserializeValue)
        });
      })
      .await;
  }

  pub async fn write_query_values<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Vec<T>, Error> {
    return self
      .exec
      .write_query_rows_f(sql, params, |rows| {
        return serde_rusqlite::from_rows(rows)
          .collect::<Result<Vec<_>, _>>()
          .map_err(Error::DeserializeValue);
      })
      .await;
  }

  /// Execute SQL statement.
  pub async fn execute(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<usize, Error> {
    return self.exec.execute(sql, params).await;
  }

  /// Batch execute provided SQL statementsi in batch.
  pub async fn execute_batch(&self, sql: impl AsRef<str> + Send + 'static) -> Result<(), Error> {
    return self.exec.execute_batch(sql).await;
  }

  pub fn attach(&self, path: &str, name: &str) -> Result<(), Error> {
    let query = format!("ATTACH DATABASE '{path}' AS {name} ");
    return self.exec.map(move |conn| {
      conn.execute(&query, ())?;
      return Ok(());
    });
  }

  pub fn detach(&self, name: &str) -> Result<(), Error> {
    let query = format!("DETACH DATABASE {name}");
    return self.exec.map(move |conn| {
      conn.execute(&query, ())?;
      return Ok(());
    });
  }

  pub async fn list_databases(&self) -> Result<Vec<Database>, Error> {
    return self
      .exec
      .call_reader(crate::sqlite::util::list_databases)
      .await;
  }

  /// Close the database connection.
  ///
  /// This is functionally equivalent to the `Drop` implementation for `Connection`. It consumes
  /// the `Connection`, but on error returns it to the caller for retry purposes.
  ///
  /// If successful, any following `close` operations performed on `Connection` copies will succeed
  /// immediately.
  ///
  /// # Failure
  ///
  /// Will return `Err` if the underlying SQLite close call fails.
  pub async fn close(self) -> Result<(), Error> {
    return self.exec.close().await;
  }
}

impl Debug for Connection {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Connection").finish()
  }
}

impl Hash for Connection {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.id().hash(state);
  }
}

impl PartialEq for Connection {
  fn eq(&self, other: &Self) -> bool {
    return self.id() == other.id();
  }
}

impl Eq for Connection {}

static UNIQUE_CONN_ID: AtomicUsize = AtomicUsize::new(0);
