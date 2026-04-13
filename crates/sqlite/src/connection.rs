use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::database::Database;
use crate::error::Error;
use crate::from_sql::FromSql;
use crate::params::Params;
use crate::rows::{Row, Rows, columns};
use crate::sqlite::connection::{ConnectionImpl, get_value, map_first};

// NOTE: We should probably decouple from the impl.
pub use crate::sqlite::connection::{ArcLockGuard, LockGuard, Options};

/// A handle to call functions in background thread.
#[derive(Clone)]
pub struct Connection {
  c: ConnectionImpl,
}

impl Connection {
  pub fn new<E>(builder: impl Fn() -> Result<rusqlite::Connection, E>) -> Result<Self, E> {
    return Self::with_opts(builder, Options::default());
  }

  pub fn with_opts<E>(
    builder: impl Fn() -> Result<rusqlite::Connection, E>,
    opt: Options,
  ) -> std::result::Result<Self, E> {
    return Ok(Self {
      c: ConnectionImpl::new(builder, opt)?,
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
        n_read_threads: Some(0),
        ..Default::default()
      },
    )?;

    assert_eq!(1, conn.threads());

    return Ok(conn);
  }

  pub fn id(&self) -> usize {
    return self.c.id();
  }

  pub fn threads(&self) -> usize {
    return self.c.threads();
  }

  pub fn write_lock(&self) -> LockGuard<'_> {
    return self.c.write_lock();
  }

  pub fn try_write_arc_lock_for(&self, duration: tokio::time::Duration) -> Option<ArcLockGuard> {
    return self.c.try_write_arc_lock_for(duration);
  }

  /// Call a function in background thread and get the result
  /// asynchronously.
  ///
  /// # Failure
  ///
  /// Will return `Err` if the database connection has been closed.
  pub async fn call<F, R>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R, Error> + Send + 'static,
    R: Send + 'static,
  {
    return self.c.call(function).await;
  }

  pub async fn call_reader<F, R>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(&rusqlite::Connection) -> Result<R, Error> + Send + 'static,
    R: Send + 'static,
  {
    return self.c.call_reader(function).await;
  }

  /// Query SQL statement.
  pub async fn read_query_rows(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Rows, Error> {
    return self
      .c
      .read_query_rows_f(sql, params, crate::rows::from_rows)
      .await;
  }

  pub async fn read_query_row(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<Row>, Error> {
    return self
      .c
      .read_query_rows_f(sql, params, |rows| {
        return map_first(rows, |row| {
          return crate::rows::from_row(row, Arc::new(columns(row.as_ref())));
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
      .c
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
      .c
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
      .c
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
    return self
      .c
      .write_query_rows_f(sql, params, crate::rows::from_rows)
      .await;
  }

  pub async fn query_row_get<T>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    index: usize,
  ) -> Result<Option<T>, Error>
  where
    T: FromSql + Send + 'static,
  {
    return self
      .c
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
      .c
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
      .c
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
    return self.c.execute(sql, params).await;
  }

  /// Batch execute provided SQL statementsi in batch.
  pub async fn execute_batch(&self, sql: impl AsRef<str> + Send + 'static) -> Result<(), Error> {
    return self.c.execute_batch(sql).await;
  }

  pub fn attach(&self, path: &str, name: &str) -> Result<(), Error> {
    let query = format!("ATTACH DATABASE '{path}' AS {name} ");
    return self.c.map(move |conn| {
      conn.execute(&query, ())?;
      return Ok(());
    });
  }

  pub fn detach(&self, name: &str) -> Result<(), Error> {
    let query = format!("DETACH DATABASE {name}");
    return self.c.map(move |conn| {
      conn.execute(&query, ())?;
      return Ok(());
    });
  }

  pub async fn list_databases(&self) -> Result<Vec<Database>, Error> {
    return self
      .c
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
    return self.c.close().await;
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
