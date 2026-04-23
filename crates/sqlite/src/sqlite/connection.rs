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
use crate::sqlite::sync::{SyncConnection, SyncConnectionTrait};
use crate::sqlite::transaction::Transaction;
use crate::sqlite::util::{columns, from_row, from_rows, get_value, map_first};

// NOTE: We should probably decouple from the impl.
pub use crate::sqlite::executor::{ArcLockGuard, LockError, LockGuard, Options};

/// A handle to call functions in background thread.
#[derive(Clone)]
pub struct Connection {
  id: usize,
  pub(crate) exec: Executor,
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

  /// Acquire write lock on the connections.
  ///
  /// NOTE: This should not be used for installing extension methods, since only the writer
  /// connection would be affected.
  /// NOTE: Current use cases:
  ///   * Installing pre-update hooks
  #[inline]
  pub fn write_lock(&self) -> Result<LockGuard<'_>, LockError> {
    return self.exec.write_lock();
  }

  /// Acquire a ref-counted write lock on the connections.
  ///
  /// NOTE: Current use cases:
  ///   * Transactions from WASM.
  #[inline]
  pub fn try_write_arc_lock_for(
    &self,
    duration: tokio::time::Duration,
  ) -> Result<ArcLockGuard, LockError> {
    return self.exec.try_write_arc_lock_for(duration);
  }

  /// Call a function on the writer thread and channel the result back asynchronously.
  ///
  /// # Failure
  ///
  /// Will return `Err` if the database connection has been closed.
  ///
  /// NOTE: use-cases:
  ///  * RecordAPI non-transaction batch inserts.
  ///  * Log-writer batch insertions (could use a different driver :shrug:).
  pub async fn call_writer<F, R, E>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(SyncConnection) -> Result<R, E> + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Error: From<E>,
  {
    return self
      .exec
      .call_writer(|conn| {
        return function(SyncConnection { conn });
      })
      .await;
  }

  // pub async fn call_reader<F, R, E>(&self, function: F) -> Result<R, Error>
  // where
  //   F: FnOnce(&rusqlite::Connection) -> Result<R, E> + Send + 'static,
  //   R: Send + 'static,
  //   E: Send + 'static,
  //   Error: From<E>,
  // {
  //   return self.exec.call_reader(function).await;
  // }

  /// Transactions
  ///
  /// Note: we us an async API rather than a sync blocking API, e.g.:
  ///
  ///    let tx = conn.transaction();
  ///
  /// To avoid blocking the calling thread.
  pub async fn transaction<F, R, E>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(Transaction) -> Result<R, E> + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Error: From<E>,
  {
    return self
      .exec
      .call_writer::<_, R, Error>(move |conn: &mut rusqlite::Connection| {
        let tx = conn.transaction()?;
        return Ok(function(Transaction::new(tx))?);
      })
      .await;
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

  pub async fn write_query_row(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<Row>, Error> {
    return self
      .exec
      .write_query_rows_f(sql, params, |rows| {
        return map_first(rows, |row| {
          return from_row(row, Arc::new(columns(row.as_ref())));
        });
      })
      .await;
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
    return self
      .exec
      .call_writer(move |conn: &mut rusqlite::Connection| {
        return SyncConnectionTrait::execute(conn, sql, params);
      })
      .await;
  }

  /// Batch execute provided SQL statementsi in batch.
  pub async fn execute_batch(&self, sql: impl AsRef<str> + Send + 'static) -> Result<(), Error> {
    return self
      .exec
      .call_writer(move |conn: &mut rusqlite::Connection| {
        return SyncConnectionTrait::execute_batch(conn, sql);
      })
      .await;
  }

  pub async fn attach(&self, path: &str, name: &str) -> Result<(), Error> {
    let query = format!("ATTACH DATABASE '{path}' AS {name} ");
    return self.exec.map(move |conn| {
      conn.execute(&query, ())?;
      return Ok(());
    });
  }

  pub async fn detach(&self, name: &str) -> Result<(), Error> {
    let query = format!("DETACH DATABASE {name}");
    return self.exec.map(move |conn| {
      conn.execute(&query, ())?;
      return Ok(());
    });
  }

  pub async fn backup(&self, path: impl AsRef<std::path::Path>) -> Result<(), Error> {
    let mut dst = rusqlite::Connection::open(path)?;
    return self
      .exec
      .call_reader(move |src_conn| -> Result<(), Error> {
        use rusqlite::backup::{Backup, StepResult};

        let backup = Backup::new(src_conn, &mut dst)?;
        let mut retries = 0;

        loop {
          match backup.step(/* num_pages= */ 128)? {
            StepResult::Done => {
              return Ok(());
            }
            StepResult::More => {
              retries = 0;
              // Just continue.
            }
            StepResult::Locked | StepResult::Busy => {
              retries += 1;
              if retries > 100 {
                return Err(Error::Other("Backup failed".into()));
              }

              // Retry.
              std::thread::sleep(std::time::Duration::from_micros(100));
            }
            r => {
              // Non-exhaustive enum.
              return Err(Error::Other(
                format!("unexpected backup step result {r:?}").into(),
              ));
            }
          }
        }
      })
      .await;
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
