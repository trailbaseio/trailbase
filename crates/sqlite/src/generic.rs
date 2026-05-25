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
use crate::r#type::ConnectionType;

// NOTE: We should probably decouple from the impl.
pub use crate::sqlite::executor::{ArcLockGuard, LockError, LockGuard};

#[derive(Clone, Debug)]
pub enum PgConnection {
  Uri(String),
  Host {
    host: Option<String>,
    port: Option<u16>,
    user: Option<String>,
    password: Option<String>,
  },
}

#[derive(Clone, Debug)]
pub struct PgOptions {
  pub connection: PgConnection,
  pub num_threads: Option<usize>,
}

#[derive(Clone)]
enum Executor {
  Sqlite(Arc<SqliteExecutor>),
  Pg(Arc<PgExecutor>),
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
  fn new(exec: Executor) -> Self {
    return Self {
      id: UNIQUE_CONN_ID.fetch_add(1, Ordering::SeqCst),
      exec,
    };
  }

  /// TODO: Should be renamed. Default to sqlite for PoC.
  pub fn with_opts<E>(
    builder: impl Fn() -> Result<rusqlite::Connection, E>,
    opts: crate::sqlite::executor::Options,
  ) -> Result<Self, Error>
  where
    Error: From<E>,
  {
    return Ok(Self::new(Executor::Sqlite(Arc::new(
      crate::sqlite::executor::Executor::new(builder, opts.clone())?,
    ))));
  }

  pub fn open_in_memory() -> Result<Self, Error> {
    let inst = Self::with_opts(
      rusqlite::Connection::open_in_memory,
      crate::sqlite::executor::Options {
        num_threads: Some(1),
        ..Default::default()
      },
    )?;
    assert_eq!(1, inst.threads());

    return Ok(inst);
  }

  pub fn pg_with_opts(opts: PgOptions) -> Result<Self, Error> {
    use postgres::{Client, NoTls};

    return Ok(Self::new(Executor::Pg(Arc::new(
      crate::pg::executor::Executor::new(
        move || -> Result<Client, Error> {
          return match &opts.connection {
            PgConnection::Uri(uri) => Ok(Client::connect(uri, NoTls)?),
            PgConnection::Host {
              host,
              port,
              user,
              password,
            } => {
              let mut conf = Client::configure();
              if let Some(host) = host {
                conf.host(host);
              }
              if let Some(port) = port {
                conf.port(*port);
              }
              if let Some(user) = user {
                conf.user(user);
              }
              if let Some(pw) = password {
                conf.password(pw);
              }

              Ok(conf.connect(NoTls)?)
            }
          };
        },
        crate::pg::executor::Options {
          num_threads: opts.num_threads,
        },
      )?,
    ))));
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

  pub fn connection_type(&self) -> ConnectionType {
    return match self.exec {
      Executor::Sqlite(_) => ConnectionType::Sqlite,
      Executor::Pg(_) => ConnectionType::Pg,
    };
  }

  #[inline]
  pub fn write_lock(&self) -> Result<LockGuard<'_>, LockError> {
    return match self.exec {
      Executor::Sqlite(ref exec) => exec.write_lock(),
      // Expected: while locking is less of a problem for PG, running sync postgres on a
      // tokio task will make the runtime panic.
      Executor::Pg(_) => {
        log::error!("Not supported: PG write lock");

        Err(LockError::NotSupported)
      }
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
      Executor::Pg(_) => {
        log::error!("Not supported: PG arc write lock");

        Err(LockError::NotSupported)
      }
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
      Executor::Pg(ref exec) => exec.query_rows_f(sql, params, pg_from_rows).await,
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
              // TODO: Coming from here, I guess.
              return trailbase_pgrow2serde::from_row(&row).map_err(|err| Error::Other(err.into()));
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
                return trailbase_pgrow2serde::from_row(&row)
                  .map_err(|err| Error::Other(err.into()));
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
          Ok(())
        })
      }
      Executor::Pg(_) => {
        log::error!("Not implemented: attach DB");

        Err(Error::NotImplemented)
      }
    };
  }

  pub async fn detach(&self, name: &str) -> Result<(), Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        let query = format!("DETACH DATABASE {name}");
        exec.map(move |conn| {
          conn.execute(&query, ())?;
          Ok(())
        })
      }
      Executor::Pg(_) => {
        log::error!("Not implemented: detach DB");

        Err(Error::NotImplemented)
      }
    };
  }

  pub async fn backup(&self, path: impl AsRef<std::path::Path>) -> Result<(), Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => {
        let mut dst = rusqlite::Connection::open(path)?;
        exec
          .call_reader(move |src_conn| -> Result<(), Error> {
            return crate::sqlite::util::backup(src_conn, &mut dst);
          })
          .await
      }
      Executor::Pg(_) => {
        log::error!("Not implemented: backup");

        Err(Error::NotImplemented)
      }
    };
  }

  pub async fn list_databases(&self) -> Result<Vec<Database>, Error> {
    return match self.exec {
      Executor::Sqlite(ref exec) => exec.call_reader(crate::sqlite::util::list_databases).await,
      Executor::Pg(_) => {
        log::error!("Not implemented: list databases");

        return Err(Error::NotImplemented);
      }
    };
  }

  /// Close the database connection.
  ///
  /// WARN: that since connections are clonable, closing one connection may affect others.
  /// Alternatively just drop the connection and the underlying connection will be cleaned
  /// up when all references have been dropped.
  pub async fn close(self) -> Result<(), Error> {
    return match self.exec {
      Executor::Sqlite(exec) => exec.close_impl(),
      Executor::Pg(exec) => exec.close_impl(),
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

pub async fn execute_batch(
  conn: &Connection,
  sql: impl AsRef<str> + Send + 'static,
) -> Result<Option<Rows>, Error> {
  return match conn.exec {
    Executor::Sqlite(ref exec) => crate::sqlite::batch::execute_batch_impl(exec, sql).await,
    Executor::Pg(ref exec) => crate::pg::util::execute_batch_impl(exec, sql).await,
  };
}

static UNIQUE_CONN_ID: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
mod tests {
  use pglite_oxide::PgliteServer;
  use serde::Deserialize;

  use super::*;
  use crate::pg::executor::build_pg_test_executor;
  use crate::{named_params, params};

  #[tokio::test]
  async fn generic_pg_poc_test() {
    let (_db, exec) = build_pg_test_executor().unwrap();
    let conn = Connection::new(Executor::Pg(Arc::new(exec)));

    // IMPORTANT: PgLite only handles a single concurrent connection.
    assert_eq!(1, conn.threads());

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
        "SELECT COUNT(*) FROM public.test_table_poc_generic WHERE data = ?1",
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

    assert_eq!(
      1,
      conn
        .execute(
          "UPDATE test_table_poc_generic SET data = 'c' WHERE data = $1",
          params!("a"),
        )
        .await
        .unwrap()
    );

    // Make sure queries returning rows fail.
    assert!(matches!(
      conn.execute("SELECT 5;", ()).await,
      Err(Error::ExecuteReturnedResults)
    ));

    // Batch succeeds (consistent with rusqlite's execute_batch).
    conn.execute_batch("SELECT 5;").await.unwrap();
  }

  #[tokio::test]
  async fn generic_connection_w_pg_test() {
    let db = PgliteServer::temporary_tcp().unwrap();
    let pg_uri = db.connection_uri();
    println!("Started PgLite: {pg_uri}");

    let conn = Connection::pg_with_opts(PgOptions {
      connection: PgConnection::Uri(pg_uri),
      num_threads: Some(1),
    })
    .unwrap();

    // IMPORTANT: PgLite only handles a single concurrent connection.
    assert_eq!(1, conn.threads());

    let rows = conn.read_query_rows("SELECT 5", ()).await.unwrap();
    let n: i64 = rows.get(0).unwrap().get(0).unwrap();
    assert_eq!(5, n);

    #[derive(Debug, Deserialize, PartialEq)]
    struct Data {
      bytes: [u8; 4],
      vec: Vec<u8>,
      text: String,
      text_null: Option<String>,
      flag: bool,
      int_null: Option<i64>,
      bool_from_int: bool,
    }
    let query = "
      SELECT
        CAST('\x05' AS bytea) AS bytes,
        CAST('\x03' AS bytea) AS vec,
        'foo' AS text,
        NULL AS text_null,
        false AS flag,
        CAST(0 AS INT8) AS int_null,
        1 AS bool_from_int
      ;";

    let data: Data = conn.read_query_value(query, ()).await.unwrap().unwrap();

    assert_eq!(
      Data {
        bytes: [5, 0, 0, 0],
        vec: vec![3],
        text: "foo".to_string(),
        text_null: None,
        flag: false,
        int_null: Some(0),
        bool_from_int: true,
      },
      data
    );
  }

  #[tokio::test]
  async fn generic_connection_w_pg_create_simple_table_test() {
    let db = PgliteServer::temporary_tcp().unwrap();
    let pg_uri = db.connection_uri();
    println!("Started PgLite: {pg_uri}");

    let conn = Connection::pg_with_opts(PgOptions {
      connection: PgConnection::Uri(pg_uri),
      num_threads: Some(1),
    })
    .unwrap();

    conn
      .execute_batch(
        "CREATE TABLE foo (
        \"bool\" BOOLEAN,
        \"uuid\" UUID,
        \"text\" TEXT
    );",
      )
      .await
      .unwrap();

    assert_eq!(
      1,
      conn
        .execute(
          r#"INSERT INTO foo ("bool", "uuid", "text") VALUES (:b, :__u, :t)"#,
          named_params! {
              ":b": true,
              ":__u": [0u8; 16],
              ":t": "test",
          },
        )
        .await
        .unwrap()
    );
  }

  #[tokio::test]
  async fn generic_connection_w_pg_create_more_complex_table_test() {
    let db = PgliteServer::temporary_tcp().unwrap();
    let pg_uri = db.connection_uri();
    println!("Started PgLite: {pg_uri}");

    let conn = Connection::pg_with_opts(PgOptions {
      connection: PgConnection::Uri(pg_uri),
      num_threads: Some(1),
    })
    .unwrap();

    conn
      .execute_batch(
        r#"
          CREATE TABLE IF NOT EXISTS _user (
            id                               UUID PRIMARY KEY NOT NULL DEFAULT (gen_random_uuid()),
            email                            TEXT NOT NULL
          );

          CREATE TABLE room (
            rid          UUID PRIMARY KEY NOT NULL DEFAULT(gen_random_uuid()),
            name         TEXT
          );

          CREATE TABLE message (
            mid          UUID PRIMARY KEY NOT NULL DEFAULT (gen_random_uuid()),
            _owner       UUID NOT NULL,
            room         UUID NOT NULL,
            data         TEXT NOT NULL DEFAULT 'empty',

            -- on user delete, tombstone it.
            FOREIGN KEY(_owner) REFERENCES _user(id) ON DELETE SET NULL,
            -- On chat room delete, delete message
            FOREIGN KEY(room) REFERENCES room(rid) ON DELETE CASCADE
          );

          CREATE TABLE room_members (
            "user"       UUID NOT NULL,
            room         UUID NOT NULL,

            FOREIGN KEY(room) REFERENCES room(rid) ON DELETE CASCADE,
            FOREIGN KEY("user") REFERENCES _user(id) ON DELETE CASCADE
          );
        "#,
      )
      .await
      .unwrap();

    let user_id = uuid::Uuid::new_v4();
    assert_eq!(
      1,
      conn
        .execute(
          "INSERT INTO _user (id, email) VALUES ($1, 'a@b.org');",
          params!(user_id.into_bytes())
        )
        .await
        .unwrap()
    );

    let room_id = uuid::Uuid::new_v4();
    assert_eq!(
      1,
      conn
        .execute(
          "INSERT INTO room (rid, name) VALUES ($1, 'test_room');",
          params!(room_id.into_bytes())
        )
        .await
        .unwrap()
    );

    assert_eq!(
      1,
      conn
        .execute(
          "INSERT INTO room_members (\"user\", room) VALUES ($1, $2);",
          params!(user_id.into_bytes(), room_id.into_bytes())
        )
        .await
        .unwrap()
    );

    let message_id = uuid::Uuid::new_v4();
    assert_eq!(
      1,
      conn
        .execute(
          "INSERT INTO message (mid, _owner, room) VALUES ($1, $2, $3);",
          params!(
            message_id.into_bytes(),
            user_id.into_bytes(),
            room_id.into_bytes(),
          )
        )
        .await
        .unwrap()
    );

    let read_rla_query = r#"
      SELECT
        CAST(((_ROW_._owner = _USER_.id OR EXISTS(SELECT 1 FROM room_members WHERE room = _ROW_.room AND "user" = _USER_.id))) AS INTEGER)
      FROM
        (SELECT CAST(:__user_id AS uuid) AS id) AS _USER_,
        (SELECT * FROM "public"."message" WHERE "mid" = CAST(:__record_id AS uuid)) AS _ROW_"#;

    assert_eq!(
      1,
      conn
        .read_query_row_get::<i64>(
          read_rla_query,
          named_params! {
              ":__record_id": message_id.into_bytes(),
              ":__user_id": user_id.into_bytes(),
          },
          0,
        )
        .await
        .unwrap()
        .unwrap()
    );

    let create_rla_query = r#"
      WITH _REQ_FIELDS_(_) AS (SELECT value FROM json_array_elements_text(:__fields))
      --
      SELECT
        CAST(
          (
            _USER_.id = _REQ_._owner AND
            EXISTS(
              SELECT 1 FROM room_members AS m WHERE m.user = _USER_.id AND m.room = _REQ_.room
            ) AND
            -- NOTE: PG doesn't support `IN (subquery)`, i.e.: `'mid' IN _REQ_FIELDS_`, like SQLite does.
            'mid' IN (SELECT * FROM _REQ_FIELDS_)
          )
          AS INTEGER)
      FROM
        (SELECT CAST(:__user_id AS uuid) AS id) AS _USER_,
        (SELECT CAST(:mid AS uuid) AS "mid", CAST(:_owner AS uuid) AS "_owner", CAST(:room AS uuid) AS "room", :data AS "data") AS _REQ_;
      "#;

    assert_eq!(
      1,
      conn
        .read_query_row_get::<i64>(
          create_rla_query,
          named_params! {
              ":__fields": r#"["mid", "_owner", "data"]"#,
              ":__user_id": user_id.into_bytes(),
              ":mid": message_id.into_bytes(),
              ":_owner": user_id.into_bytes(),
              ":room": room_id.into_bytes(),
              ":data": "foo",
          },
          0,
        )
        .await
        .unwrap()
        .unwrap()
    );
  }

  #[tokio::test]
  async fn pg_lite_test() {
    let (_db, exec) = build_pg_test_executor().unwrap();
    let conn = Connection::new(Executor::Pg(Arc::new(exec)));

    {
      // Make sure `pglite-oxide`'s RNG works correctly.
      // https://github.com/f0rr0/pglite-oxide/issues/29
      let uuid0: [u8; 16] = conn
        .read_query_value("SELECT gen_random_uuid()", ())
        .await
        .unwrap()
        .unwrap();

      let uuid1: [u8; 16] = conn
        .read_query_value("SELECT gen_random_uuid()", ())
        .await
        .unwrap()
        .unwrap();

      assert_ne!(uuid0, uuid1);
    }
  }

  #[tokio::test]
  async fn pg_int_test() {
    let (_db, exec) = build_pg_test_executor().unwrap();
    let conn = Connection::new(Executor::Pg(Arc::new(exec)));

    conn
      .execute_batch(
        "
        CREATE TABLE int_table (
          \"s\"     serial,
          \"i2\"    int2,
          \"i4\"    int4,
          \"i8\"    int8
        );
        ",
      )
      .await
      .unwrap();

    for col in ["s", "i2", "i4", "i8"] {
      conn
        .execute(
          format!("INSERT INTO int_table({col}) VALUES ($1);"),
          [Value::Integer(5)],
        )
        .await
        .unwrap();

      let select = format!("SELECT {col} FROM int_table WHERE {col} IS NOT NULL");
      conn
        .read_query_row_get::<Value>(select.clone(), (), 0)
        .await
        .unwrap()
        .unwrap();

      conn
        .read_query_row_get::<i64>(select.clone(), (), 0)
        .await
        .unwrap()
        .unwrap();
    }
  }

  #[tokio::test]
  async fn pg_float_test() {
    let (_db, exec) = build_pg_test_executor().unwrap();
    let conn = Connection::new(Executor::Pg(Arc::new(exec)));

    conn
      .execute_batch(
        "
        CREATE TABLE float_table (
          \"f4\"    float4,
          \"f8\"    float8
        );
        ",
      )
      .await
      .unwrap();

    for col in ["f4", "f8"] {
      for v in [Value::Real(5.0), Value::Integer(5)] {
        conn
          .execute(format!("INSERT INTO float_table({col}) VALUES ($1);"), [v])
          .await
          .unwrap();
      }

      let select = format!("SELECT {col} FROM float_table WHERE {col} IS NOT NULL");
      conn
        .read_query_row_get::<Value>(select.clone(), (), 0)
        .await
        .unwrap()
        .unwrap();

      conn
        .read_query_row_get::<f64>(select.clone(), (), 0)
        .await
        .unwrap()
        .unwrap();
    }
  }

  #[tokio::test]
  async fn pg_uuids_test() {
    let (_db, exec) = build_pg_test_executor().unwrap();
    let conn = Connection::new(Executor::Pg(Arc::new(exec)));

    let uuid: Vec<u8> = conn
      .read_query_row_get("SELECT uuid_generate_v7();", (), 0)
      .await
      .unwrap()
      .unwrap();

    let version: i64 = conn
      .read_query_row_get("SELECT uuid_extract_version(:id)", [Value::Blob(uuid)], 0)
      .await
      .unwrap()
      .unwrap();

    assert_eq!(7, version);

    conn
      .execute_batch(
        "
        CREATE TABLE table_w_uuid (
          \"user\"    UUID PRIMARY KEY NOT NULL,
          \"data\"    TEXT
        );
        ",
      )
      .await
      .unwrap();

    const INSERT: &str = "INSERT INTO table_w_uuid (\"user\") VALUES ($1);";
    conn
      .execute(
        INSERT,
        (Value::Blob(uuid::Uuid::new_v4().into_bytes().into()),),
      )
      .await
      .unwrap();

    // We could support TEXT UUIDs in Value's ToSql impl, but we don't.
    assert!(
      conn
        .execute(INSERT, params!(uuid::Uuid::new_v4().to_string()))
        .await
        .is_err()
    );

    // NOTE: `tid`s in PG re a tuple of (block number, row number).
    let _ctid: i64 = conn
      .write_query_row_get(
        "INSERT INTO table_w_uuid (\"user\", data) VALUES (:user, :data) RETURNING ctid, \"user\"",
        named_params! {
            ":user": uuid::Uuid::new_v4().into_bytes().to_vec(),
            ":data": "test",
        },
        0,
      )
      .await
      .unwrap()
      .unwrap();
  }

  #[tokio::test]
  async fn pg_json_test() {
    let (_db, exec) = build_pg_test_executor().unwrap();
    let conn = Connection::new(Executor::Pg(Arc::new(exec)));

    conn
      .execute_batch(
        "
        CREATE TABLE t (
          id     SERIAL PRIMARY KEY,
          jt     JSON,
          jb     JSONB
        );

        INSERT INTO t (jt, jb) VALUES ('{\"a\": 5}', '[]');
        ",
      )
      .await
      .unwrap();

    let json_text: String = conn
      .read_query_row_get("SELECT jt FROM t;", (), 0)
      .await
      .unwrap()
      .unwrap();

    assert_eq!("{\"a\": 5}", json_text);

    let json_binary: String = conn
      .read_query_row_get("SELECT jb FROM t;", (), 0)
      .await
      .unwrap()
      .unwrap();

    assert_eq!("[]", json_binary);

    conn
      .execute(
        "INSERT INTO t (jt, jb) VALUES ($1, $2);",
        params!("[]", "{}"),
      )
      .await
      .unwrap();
  }

  #[tokio::test]
  async fn pg_tid_test() {
    let (_db, exec) = build_pg_test_executor().unwrap();
    let conn = Connection::new(Executor::Pg(Arc::new(exec)));

    conn
      .execute_batch(
        "
        CREATE TABLE t (
          id     SERIAL PRIMARY KEY,
          data   TEXT
        );

        INSERT INTO t (data) VALUES ('a'), ('b');
        ",
      )
      .await
      .unwrap();

    let tid: Value = conn
      .read_query_row_get("SELECT ctid FROM t WHERE data = 'b'", (), 0)
      .await
      .unwrap()
      .unwrap();

    log::info!("TID {tid:?}");

    let data: String = conn
      .read_query_row_get("SELECT data FROM t WHERE ctid = :id", [tid], 0)
      .await
      .unwrap()
      .unwrap();

    assert_eq!("b", data);
  }

  #[tokio::test]
  async fn pg_trigger_test() {
    let (_db, exec) = build_pg_test_executor().unwrap();
    let conn = Connection::new(Executor::Pg(Arc::new(exec)));

    let column_name = "test";
    let unqualified_name = "tt";
    let db = "public";
    let table_name = format!("{db}.{unqualified_name}");

    conn
      .execute_batch(format!(
        "
          CREATE OR REPLACE FUNCTION UNIXEPOCH() RETURNS INT8 AS $$
            BEGIN
              RETURN EXTRACT(EPOCH FROM CURRENT_TIMESTAMP);
            END;
          $$ LANGUAGE plpgsql;

          CREATE TABLE _file_deletions (
            id                           SERIAL PRIMARY KEY NOT NULL,
            deleted                      INTEGER NOT NULL DEFAULT (UNIXEPOCH()),

            -- Cleanup metadata
            attempts                     INTEGER NOT NULL DEFAULT 0,
            errors                       TEXT,

            -- Which record contained the file.
            table_name                   TEXT NOT NULL,
            record_rowid                 TID NOT NULL,
            column_name                  TEXT NOT NULL,

            -- File metadata, including id (path).
            --
            -- IMPORTANT: non-binary `JSON` type does not support comparisons.
            \"json\"                     JSONB NOT NULL
          );

          CREATE TABLE {table_name} ({column_name} JSONB);
          "
      ))
      .await
      .unwrap();

    conn
      .execute_batch(format!(
        "
          CREATE FUNCTION \"__{unqualified_name}__{column_name}__trigger_fun\"() RETURNS TRIGGER AS $$
            BEGIN
              INSERT INTO _file_deletions (table_name, record_rowid, column_name, json) VALUES
                ('{table_name}', OLD.ctid, '{column_name}', \"{column_name}\");
            END;
          $$ LANGUAGE plpgsql;

          -- DROP TRIGGER IF EXISTS \"__{unqualified_name}__{column_name}__update_trigger\" ON {table_name};

          CREATE OR REPLACE TRIGGER \"__{unqualified_name}__{column_name}__update_trigger\"
            AFTER UPDATE ON {table_name} FOR EACH ROW
            WHEN (OLD.{column_name} IS NOT NULL AND OLD.{column_name} != NEW.{column_name})
            EXECUTE FUNCTION \"__{unqualified_name}__{column_name}__trigger_fun\"();

          CREATE OR REPLACE TRIGGER \"__{unqualified_name}__{column_name}__delete_trigger\"
            AFTER DELETE ON {table_name} FOR EACH ROW
            WHEN (OLD.{column_name} IS NOT NULL)
            EXECUTE FUNCTION \"__{unqualified_name}__{column_name}__trigger_fun\"();
          ",
      ))
      .await
      .unwrap();
  }
}
