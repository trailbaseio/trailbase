use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use postgres::fallible_iterator::FallibleIterator;

use crate::Value;
use crate::database::Database;
use crate::error::Error;
use crate::from_sql::FromSql;
use crate::params::Params;
use crate::pg::executor::Executor as PgExecutor;
use crate::pg::util::{
  columns as pg_columns, from_row as pg_from_row, from_rows as pg_from_rows,
  map_first as pg_map_first,
};
use crate::rows::{Row, Rows};
use crate::sqlite::executor::Executor as SqliteExecutor;
use crate::sqlite::util::{
  columns as sqlite_columns, from_row as sqlite_from_row, from_rows as sqlite_from_rows, get_value,
  map_first as sqlite_map_first,
};
use crate::traits::{
  SyncConnection as SyncConnectionTrait, SyncTransaction as SyncTransactionTrait,
};

// NOTE: We should probably decouple from the impl.
pub use crate::sqlite::executor::{ArcLockGuard, LockError, LockGuard};

#[derive(Clone)]
pub enum Executor {
  Sqlite(SqliteExecutor),
  Pg(PgExecutor),
}

/// A handle to call functions in background thread.
#[allow(unused)]
#[derive(Clone)]
pub struct Connection {
  id: usize,
  exec: Executor,
}

#[allow(unused)]
impl Connection {
  pub fn new<E>(builder: impl Fn() -> Result<Executor, E>) -> Result<Self, Error>
  where
    Error: From<E>,
  {
    return Ok(Self {
      id: UNIQUE_CONN_ID.fetch_add(1, Ordering::SeqCst),
      exec: builder()?,
    });
  }

  pub fn id(&self) -> usize {
    return self.id;
  }

  pub fn threads(&self) -> usize {
    return match self.exec {
      Executor::Sqlite(ref exec) => exec.threads(),
      Executor::Pg(ref exec) => exec.threads(),
    };
  }

  #[inline]
  pub fn write_lock(&self) -> Result<LockGuard<'_>, LockError> {
    return match self.exec {
      Executor::Sqlite(ref exec) => exec.write_lock(),
      // Expected: while locking is less of a problem for PG, running sync postgres on a
      // tokio task will make the runtime panic.
      Executor::Pg(_) => Err(LockError::NotSupported),
    };
  }

  #[inline]
  pub fn try_write_arc_lock_for(
    &self,
    duration: tokio::time::Duration,
  ) -> Result<ArcLockGuard, LockError> {
    return match self.exec {
      Executor::Sqlite(ref exec) => exec.try_write_arc_lock_for(duration),
      // Expected: while locking is less of a problem for PG, running sync postgres on a
      // tokio task will make the runtime panic.
      Executor::Pg(_) => Err(LockError::NotSupported),
    };
  }

  pub async fn call_writer<F, R, E>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(SyncConnection) -> Result<R, E> + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Error: From<E>,
  {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .call_writer(|conn| {
            return function(SyncConnection::Sqlite(conn));
          })
          .await
      }
      Executor::Pg(ref exec) => {
        exec
          .call(|client| {
            return function(SyncConnection::Pg(client));
          })
          .await
      }
    };
  }

  pub async fn transaction<F, R, E>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(Transaction) -> Result<R, E> + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Error: From<E>,
  {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .call_writer::<_, R, Error>(move |conn: &mut rusqlite::Connection| {
            let tx = conn.transaction()?;
            return Ok(function(Transaction::Sqlite(tx))?);
          })
          .await
      }
      Executor::Pg(ref exec) => {
        exec
          .call::<_, R, Error>(move |conn: &mut postgres::Client| {
            let tx = conn.transaction()?;
            return Ok(function(Transaction::Pg(tx))?);
          })
          .await
      }
    };
  }

  pub async fn read_query_rows(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Rows, Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => exec.read_query_rows_f(sql, params, sqlite_from_rows).await,
      Executor::Pg(_) => self.write_query_rows(sql, params).await,
    };
  }

  pub async fn read_query_row(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<Row>, Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .read_query_rows_f(sql, params, |rows| {
            return sqlite_map_first(rows, |row| {
              return sqlite_from_row(row, Arc::new(sqlite_columns(row.as_ref())));
            });
          })
          .await
      }
      Executor::Pg(_) => self.write_query_row(sql, params).await,
    };
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
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .read_query_rows_f(sql, params, move |rows| {
            return sqlite_map_first(rows, move |row| {
              return get_value(row, index);
            });
          })
          .await
      }
      Executor::Pg(_) => self.write_query_row_get(sql, params, index).await,
    };
  }

  pub async fn read_query_value<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<T>, Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .read_query_rows_f(sql, params, |rows| {
            return sqlite_map_first(rows, move |row| {
              serde_rusqlite::from_row(row).map_err(Error::DeserializeValue)
            });
          })
          .await
      }
      Executor::Pg(_) => self.write_query_value(sql, params).await,
    };
  }

  pub async fn read_query_values<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Vec<T>, Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .read_query_rows_f(sql, params, |rows| {
            return serde_rusqlite::from_rows(rows)
              .collect::<Result<Vec<_>, _>>()
              .map_err(Error::DeserializeValue);
          })
          .await
      }
      Executor::Pg(_) => self.write_query_values(sql, params).await,
    };
  }

  pub async fn write_query_rows(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Rows, Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => exec.write_query_rows_f(sql, params, sqlite_from_rows).await,
      Executor::Pg(ref exec) => exec.query_rows_f(sql, params, pg_from_rows).await,
    };
  }

  pub async fn write_query_row(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<Row>, Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .write_query_rows_f(sql, params, |rows| {
            return sqlite_map_first(rows, |row| {
              return sqlite_from_row(row, Arc::new(sqlite_columns(row.as_ref())));
            });
          })
          .await
      }
      Executor::Pg(ref exec) => {
        exec
          .query_rows_f(sql, params, |rows| {
            return pg_map_first(rows, |row| {
              return pg_from_row(&row, Arc::new(pg_columns(&row)));
            });
          })
          .await
      }
    };
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
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .write_query_rows_f(sql, params, move |rows| {
            return sqlite_map_first(rows, move |row| {
              return get_value(row, index);
            });
          })
          .await
      }
      Executor::Pg(ref exec) => {
        exec
          .query_rows_f(sql, params, |rows| {
            return pg_map_first(rows, |row| {
              let value = row.try_get::<'_, usize, Value>(0)?;
              return Ok(T::column_result((&value).into())?);
            });
          })
          .await
      }
    };
  }

  pub async fn write_query_value<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<T>, Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .write_query_rows_f(sql, params, |rows| {
            return sqlite_map_first(rows, |row| {
              serde_rusqlite::from_row(row).map_err(Error::DeserializeValue)
            });
          })
          .await
      }
      Executor::Pg(ref exec) => {
        exec
          .query_rows_f(sql, params, |row_iter| {
            return pg_map_first(row_iter, |row| {
              return pgrow2serde::from_row(&row).map_err(|err| Error::Other(err.into()));
            });
          })
          .await
      }
    };
  }

  pub async fn write_query_values<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Vec<T>, Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .write_query_rows_f(sql, params, |rows| {
            return serde_rusqlite::from_rows(rows)
              .collect::<Result<Vec<_>, _>>()
              .map_err(Error::DeserializeValue);
          })
          .await
      }
      Executor::Pg(ref exec) => {
        exec
          .query_rows_f(sql, params, |row_iter| {
            return row_iter
              .iterator()
              .map(|row| {
                let row = row.map_err(|err| Error::Other(err.into()))?;
                return pgrow2serde::from_row(&row).map_err(|err| Error::Other(err.into()));
              })
              .collect();
          })
          .await
      }
    };
  }

  pub async fn execute(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<usize, Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .call_writer(move |conn: &mut rusqlite::Connection| {
            return SyncConnectionTrait::execute(conn, sql, params);
          })
          .await
      }
      Executor::Pg(ref exec) => {
        exec
          .call(move |client| {
            return SyncConnectionTrait::execute(client, sql, params);
          })
          .await
      }
    };
  }

  pub async fn execute_batch(&self, sql: impl AsRef<str> + Send + 'static) -> Result<(), Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        exec
          .call_writer(move |conn: &mut rusqlite::Connection| {
            return SyncConnectionTrait::execute_batch(conn, sql);
          })
          .await
      }
      Executor::Pg(ref exec) => {
        exec
          .call(move |client| {
            return SyncConnectionTrait::execute_batch(client, sql);
          })
          .await
      }
    };
  }

  pub async fn attach(&self, path: &str, name: &str) -> Result<(), Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        let query = format!("ATTACH DATABASE '{path}' AS {name} ");
        exec.map(move |conn| {
          conn.execute(&query, ())?;
          return Ok(());
        })
      }
      Executor::Pg(_) => {
        // TBD
        return Err(Error::NotSupported);
      }
    };
  }

  pub async fn detach(&self, name: &str) -> Result<(), Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        let query = format!("DETACH DATABASE {name}");
        exec.map(move |conn| {
          conn.execute(&query, ())?;
          return Ok(());
        })
      }
      Executor::Pg(_) => {
        // TBD
        return Err(Error::NotSupported);
      }
    };
  }

  // pub async fn backup(&self, path: impl AsRef<std::path::Path>) -> Result<(), Error> {
  //   let mut dst = rusqlite::Connection::open(path)?;
  //   return self
  //     .exec
  //     .call_reader(move |src_conn| -> Result<(), Error> {
  //       use rusqlite::backup::{Backup, StepResult};
  //
  //       let backup = Backup::new(src_conn, &mut dst)?;
  //       let mut retries = 0;
  //
  //       loop {
  //         match backup.step(/* num_pages= */ 128)? {
  //           StepResult::Done => {
  //             return Ok(());
  //           }
  //           StepResult::More => {
  //             retries = 0;
  //             // Just continue.
  //           }
  //           StepResult::Locked | StepResult::Busy => {
  //             retries += 1;
  //             if retries > 100 {
  //               return Err(Error::Other("Backup failed".into()));
  //             }
  //
  //             // Retry.
  //             std::thread::sleep(std::time::Duration::from_micros(100));
  //           }
  //           r => {
  //             // Non-exhaustive enum.
  //             return Err(Error::Other(
  //               format!("unexpected backup step result {r:?}").into(),
  //             ));
  //           }
  //         }
  //       }
  //     })
  //     .await;
  // }

  pub async fn list_databases(&self) -> Result<Vec<Database>, Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => exec.call_reader(crate::sqlite::util::list_databases).await,
      Executor::Pg(_) => {
        // TBD
        return Err(Error::NotSupported);
      }
    };
  }

  pub async fn close(self) -> Result<(), Error> {
    return match self.exec {
      Executor::Sqlite(exec) => exec.close().await,
      Executor::Pg(exec) => exec.close(),
    };
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

pub enum SyncConnection<'a> {
  Sqlite(&'a mut rusqlite::Connection),
  Pg(&'a mut postgres::Client),
}

impl<'a> SyncConnectionTrait for SyncConnection<'a> {
  #[inline]
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    return match self {
      Self::Sqlite(conn) => SyncConnectionTrait::query_row(*conn, sql, params),
      Self::Pg(client) => SyncConnectionTrait::query_row(*client, sql, params),
    };
  }

  #[inline]
  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    return match self {
      Self::Sqlite(conn) => SyncConnectionTrait::query_rows(*conn, sql, params),
      Self::Pg(client) => SyncConnectionTrait::query_rows(*client, sql, params),
    };
  }

  #[inline]
  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    return match self {
      Self::Sqlite(conn) => SyncConnectionTrait::execute(*conn, sql, params),
      Self::Pg(client) => SyncConnectionTrait::execute(*client, sql, params),
    };
  }

  #[inline]
  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    return match self {
      Self::Sqlite(conn) => SyncConnectionTrait::execute_batch(*conn, sql),
      Self::Pg(client) => SyncConnectionTrait::execute_batch(*client, sql),
    };
  }
}

pub enum Transaction<'a> {
  Sqlite(rusqlite::Transaction<'a>),
  Pg(postgres::Transaction<'a>),
}

#[allow(unused)]
impl<'a> SyncConnectionTrait for Transaction<'a> {
  #[inline]
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    return match self {
      Self::Sqlite(tx) => SyncConnectionTrait::query_row(tx, sql, params),
      Self::Pg(tx) => SyncConnectionTrait::query_row(tx, sql, params),
    };
  }

  #[inline]
  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    return match self {
      Self::Sqlite(tx) => SyncConnectionTrait::query_rows(tx, sql, params),
      Self::Pg(tx) => SyncConnectionTrait::query_rows(tx, sql, params),
    };
  }

  #[inline]
  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    return match self {
      Self::Sqlite(tx) => SyncConnectionTrait::execute(tx, sql, params),
      Self::Pg(tx) => SyncConnectionTrait::execute(tx, sql, params),
    };
  }

  #[inline]
  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    return match self {
      Self::Sqlite(tx) => SyncConnectionTrait::execute_batch(tx, sql),
      Self::Pg(tx) => SyncConnectionTrait::execute_batch(tx, sql),
    };
  }
}

#[allow(unused)]
impl<'a> SyncTransactionTrait for Transaction<'a> {
  fn commit(self) -> Result<(), Error> {
    return match self {
      Self::Sqlite(tx) => crate::sqlite::transaction::Transaction { tx }.commit(),
      Self::Pg(tx) => SyncTransactionTrait::commit(tx),
    };
  }

  fn rollback(self) -> Result<(), Error> {
    return match self {
      Self::Sqlite(tx) => crate::sqlite::transaction::Transaction { tx }.rollback(),
      Self::Pg(tx) => SyncTransactionTrait::rollback(tx),
    };
  }

  fn expand_sql(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<String>, Error> {
    return match self {
      Self::Sqlite(tx) => {
        let mut stmt = tx.prepare(sql.as_ref())?;
        params.bind(&mut stmt)?;
        return Ok(stmt.expanded_sql());
      }
      Self::Pg(tx) => SyncTransactionTrait::expand_sql(tx, sql, params),
    };
  }
}

static UNIQUE_CONN_ID: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
mod tests {
  use super::*;
  use postgres::{Client, NoTls};

  fn build_executor() -> Result<crate::pg::executor::Executor, Error> {
    return crate::pg::executor::Executor::new(
      || {
        return Client::configure()
          .host("localhost")
          .port(5432)
          .user("postgres")
          .password("example")
          .connect(NoTls);
      },
      crate::pg::executor::Options {
        num_threads: Some(2),
      },
    );
  }

  #[tokio::test]
  async fn generic_pg_poc_test() {
    let conn = Connection::new(|| -> Result<_, Error> {
      return Ok(Executor::Pg(build_executor()?));
    })
    .unwrap();

    assert_eq!(2, conn.threads());

    conn
      .call_writer(|mut client| {
        return client.execute_batch(
          "
            CREATE TABLE IF NOT EXISTS test_table_poc_generic(
              id     SERIAL PRIMARY KEY,
              data   TEXT NOT NULL
            );

            INSERT INTO test_table_poc_generic (data) VALUES ('a'), ('b');
          ",
        );
      })
      .await
      .unwrap();

    let row = conn
      .read_query_row(
        "SELECT COUNT(*) FROM test_table_poc_generic WHERE data = ?1",
        ("a".to_string(),),
      )
      .await
      .unwrap()
      .unwrap();

    let count0: i64 = row.get(0).unwrap();
    assert!(count0 > 0, "{row:?}");

    let count1: i64 = conn
      .read_query_row_get(
        "SELECT COUNT(*) FROM test_table_poc_generic WHERE data = $1",
        ("a".to_string(),),
        0,
      )
      .await
      .unwrap()
      .unwrap();

    assert_eq!(count0, count1);
  }
}
