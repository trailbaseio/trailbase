use kanal::{Receiver, Sender};
use log::*;
use parking_lot::RwLock;
use rusqlite::fallible_iterator::FallibleIterator;
use rusqlite::hooks::PreUpdateCase;
use rusqlite::types::Value;
use std::ops::{Deref, DerefMut};
use std::{
  fmt::{self, Debug},
  sync::Arc,
};
use tokio::sync::oneshot;

use crate::error::Error;
pub use crate::params::Params;
use crate::rows::{Column, columns};
pub use crate::rows::{Row, Rows};

#[macro_export]
macro_rules! params {
    () => {
        [] as [$crate::params::ToSqlType]
    };
    ($($param:expr),+ $(,)?) => {
        [$(Into::<$crate::params::ToSqlType>::into($param)),+]
    };
}

#[macro_export]
macro_rules! named_params {
    () => {
        [] as [(&str, $crate::params::ToSqlType)]
    };
    ($($param_name:literal: $param_val:expr),+ $(,)?) => {
        [$(($param_name as &str, Into::<$crate::params::ToSqlType>::into($param_val))),+]
    };
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Database {
  pub seq: u8,
  pub name: String,
}

#[derive(Default)]
struct ConnectionVec(Vec<rusqlite::Connection>);

// NOTE: We must never access the same connection concurrently even as immutable &Connection, due
// to intrinsic statement cache. We can ensure this by uniquely assigning one connection to each
// thread.
unsafe impl Sync for ConnectionVec {}

/// The result returned on method calls in this crate.
pub type Result<T> = std::result::Result<T, Error>;

enum Message {
  RunMut(Box<dyn FnOnce(&mut rusqlite::Connection) + Send + 'static>),
  RunConst(Box<dyn FnOnce(&rusqlite::Connection) + Send + 'static>),
  Terminate,
}

#[derive(Clone)]
pub struct Options {
  pub busy_timeout: std::time::Duration,
  pub n_read_threads: usize,
}

impl Default for Options {
  fn default() -> Self {
    return Self {
      busy_timeout: std::time::Duration::from_secs(5),
      n_read_threads: 0,
    };
  }
}

/// A handle to call functions in background thread.
#[derive(Clone)]
pub struct Connection {
  reader: Sender<Message>,
  writer: Sender<Message>,
  conns: Arc<RwLock<ConnectionVec>>,
}

impl Connection {
  pub fn new<E>(
    builder: impl Fn() -> std::result::Result<rusqlite::Connection, E>,
    opt: Option<Options>,
  ) -> std::result::Result<Self, E> {
    let new_conn = || -> std::result::Result<rusqlite::Connection, E> {
      let conn = builder()?;
      if let Some(timeout) = opt.as_ref().map(|o| o.busy_timeout) {
        conn.busy_timeout(timeout).expect("busy timeout failed");
      }
      return Ok(conn);
    };

    let write_conn = new_conn()?;
    let in_memory = write_conn.path().is_none_or(|s| {
      // Returns empty string for in-memory databases.
      return !s.is_empty();
    });

    let n_read_threads: i64 = match (in_memory, opt.as_ref().map_or(0, |o| o.n_read_threads)) {
      (true, _) => {
        // We cannot share an in-memory database across threads, they're all independent.
        0
      }
      (false, 1) => {
        warn!("A single reader thread won't improve performance, falling back to 0.");
        0
      }
      (false, n) => {
        if let Ok(max) = std::thread::available_parallelism()
          && n > max.get()
        {
          warn!(
            "Num read threads '{n}' exceeds hardware parallelism: {}",
            max.get()
          );
        }
        n as i64
      }
    };

    let conns = Arc::new(RwLock::new(ConnectionVec({
      let mut conns = vec![write_conn];
      for _ in 0..(n_read_threads - 1).max(0) {
        conns.push(new_conn()?);
      }
      conns
    })));

    assert_eq!(n_read_threads.max(1) as usize, conns.read().0.len());

    // Spawn writer.
    let (shared_write_sender, shared_write_receiver) = kanal::unbounded::<Message>();
    {
      let conns = conns.clone();
      std::thread::Builder::new()
        .name("tb-sqlite-writer".to_string())
        .spawn(move || event_loop(0, conns, shared_write_receiver))
        .expect("startup");
    }

    // Spawn readers.
    let shared_read_sender = if n_read_threads > 0 {
      let (shared_read_sender, shared_read_receiver) = kanal::unbounded::<Message>();
      for i in 0..n_read_threads {
        // NOTE: read and writer threads are sharing the first conn, given they're mutually
        // exclusive.
        let index = i as usize;
        let shared_read_receiver = shared_read_receiver.clone();
        let conns = conns.clone();

        std::thread::Builder::new()
          .name(format!("tb-sqlite-reader-{index}"))
          .spawn(move || event_loop(index, conns, shared_read_receiver))
          .expect("startup");
      }
      shared_read_sender
    } else {
      shared_write_sender.clone()
    };

    debug!(
      "Opened SQLite DB '{}' with {n_read_threads} reader threads",
      conns.read().0[0].path().unwrap_or("<in-memory>")
    );

    return Ok(Self {
      reader: shared_read_sender,
      writer: shared_write_sender,
      conns,
    });
  }

  pub fn from_connection_test_only(conn: rusqlite::Connection) -> Self {
    use parking_lot::lock_api::RwLock;

    let (shared_write_sender, shared_write_receiver) = kanal::unbounded::<Message>();
    let conns = Arc::new(RwLock::new(ConnectionVec(vec![conn])));
    let conns_clone = conns.clone();
    std::thread::spawn(move || event_loop(0, conns_clone, shared_write_receiver));

    return Self {
      reader: shared_write_sender.clone(),
      writer: shared_write_sender,
      conns,
    };
  }

  /// Open a new connection to an in-memory SQLite database.
  ///
  /// # Failure
  ///
  /// Will return `Err` if the underlying SQLite open call fails.
  pub fn open_in_memory() -> Result<Self> {
    return Self::new(|| Ok(rusqlite::Connection::open_in_memory()?), None);
  }

  #[inline]
  pub fn write_lock(&self) -> LockGuard<'_> {
    return LockGuard {
      guard: self.conns.write(),
    };
  }

  #[inline]
  pub fn try_write_lock_for(&self, duration: tokio::time::Duration) -> Option<LockGuard<'_>> {
    return self
      .conns
      .try_write_for(duration)
      .map(|guard| LockGuard { guard });
  }

  #[inline]
  pub fn write_arc_lock(&self) -> ArcLockGuard {
    return ArcLockGuard {
      guard: self.conns.write_arc(),
    };
  }

  #[inline]
  pub fn try_write_arc_lock_for(&self, duration: tokio::time::Duration) -> Option<ArcLockGuard> {
    return self
      .conns
      .try_write_arc_for(duration)
      .map(|guard| ArcLockGuard { guard });
  }

  /// Call a function in background thread and get the result
  /// asynchronously.
  ///
  /// # Failure
  ///
  /// Will return `Err` if the database connection has been closed.
  #[inline]
  pub async fn call<F, R>(&self, function: F) -> Result<R>
  where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R> + Send + 'static,
    R: Send + 'static,
  {
    // return call_impl(&self.writer, function).await;
    let (sender, receiver) = oneshot::channel::<Result<R>>();

    self
      .writer
      .send(Message::RunMut(Box::new(move |conn| {
        if !sender.is_closed() {
          let _ = sender.send(function(conn));
        }
      })))
      .map_err(|_| Error::ConnectionClosed)?;

    receiver.await.map_err(|_| Error::ConnectionClosed)?
  }

  #[inline]
  pub fn call_and_forget(&self, function: impl FnOnce(&rusqlite::Connection) + Send + 'static) {
    let _ = self
      .writer
      .send(Message::RunMut(Box::new(move |conn| function(conn))));
  }

  #[inline]
  pub async fn call_reader<F, R>(&self, function: F) -> Result<R>
  where
    F: FnOnce(&rusqlite::Connection) -> Result<R> + Send + 'static,
    R: Send + 'static,
  {
    let (sender, receiver) = oneshot::channel::<Result<R>>();

    self
      .reader
      .send(Message::RunConst(Box::new(move |conn| {
        if !sender.is_closed() {
          let _ = sender.send(function(conn));
        }
      })))
      .map_err(|_| Error::ConnectionClosed)?;

    receiver.await.map_err(|_| Error::ConnectionClosed)?
  }

  /// Query SQL statement.
  pub async fn read_query_rows(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Rows> {
    return self
      .call_reader(move |conn: &rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;
        assert!(stmt.readonly());

        params.bind(&mut stmt)?;
        let rows = stmt.raw_query();
        Ok(Rows::from_rows(rows)?)
      })
      .await;
  }

  pub async fn write_query_rows(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Rows> {
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;

        params.bind(&mut stmt)?;
        let rows = stmt.raw_query();
        Ok(Rows::from_rows(rows)?)
      })
      .await;
  }

  pub async fn read_query_row(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<Row>> {
    return self
      .read_query_row_f(sql, params, |row| {
        return Row::from_row(row, Arc::new(columns(row.as_ref())));
      })
      .await;
  }

  #[inline]
  pub async fn query_row_f<T, E>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    f: impl (FnOnce(&rusqlite::Row<'_>) -> std::result::Result<T, E>) + Send + 'static,
  ) -> Result<Option<T>>
  where
    T: Send + 'static,
    crate::error::Error: From<E>,
  {
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;
        params.bind(&mut stmt)?;

        let mut rows = stmt.raw_query();

        if let Some(row) = rows.next()? {
          return Ok(Some(f(row)?));
        }
        Ok(None)
      })
      .await;
  }

  #[inline]
  pub async fn read_query_row_f<T, E>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    f: impl (FnOnce(&rusqlite::Row<'_>) -> std::result::Result<T, E>) + Send + 'static,
  ) -> Result<Option<T>>
  where
    T: Send + 'static,
    crate::error::Error: From<E>,
  {
    return self
      .call_reader(move |conn: &rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;
        assert!(stmt.readonly());

        params.bind(&mut stmt)?;

        let mut rows = stmt.raw_query();

        if let Some(row) = rows.next()? {
          return Ok(Some(f(row)?));
        }
        Ok(None)
      })
      .await;
  }

  pub async fn read_query_value<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<T>> {
    return self
      .read_query_row_f(sql, params, serde_rusqlite::from_row)
      .await;
  }

  pub async fn write_query_value<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<T>> {
    return self
      .query_row_f(sql, params, serde_rusqlite::from_row)
      .await;
  }

  pub async fn read_query_values<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Vec<T>> {
    return self
      .call_reader(move |conn: &rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;
        assert!(stmt.readonly());

        params.bind(&mut stmt)?;
        let mut rows = stmt.raw_query();

        let mut values = vec![];
        while let Some(row) = rows.next()? {
          values.push(serde_rusqlite::from_row(row)?);
        }
        return Ok(values);
      })
      .await;
  }

  pub async fn write_query_values<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Vec<T>> {
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;

        params.bind(&mut stmt)?;
        let mut rows = stmt.raw_query();

        let mut values = vec![];
        while let Some(row) = rows.next()? {
          values.push(serde_rusqlite::from_row(row)?);
        }
        return Ok(values);
      })
      .await;
  }

  /// Execute SQL statement.
  pub async fn execute(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<usize> {
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;
        params.bind(&mut stmt)?;

        let n = stmt.raw_execute()?;

        return Ok(n);
      })
      .await;
  }

  /// Batch execute SQL statements and return rows of last statement.
  pub async fn execute_batch(&self, sql: impl AsRef<str> + Send + 'static) -> Result<Option<Rows>> {
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let batch = rusqlite::Batch::new(conn, sql.as_ref());

        let mut p = batch.peekable();
        while let Some(mut stmt) = p.next()? {
          let mut rows = stmt.raw_query();
          let row = rows.next()?;

          match p.peek()? {
            Some(_) => {}
            None => {
              if let Some(row) = row {
                let cols: Arc<Vec<Column>> = Arc::new(columns(row.as_ref()));

                let mut result = vec![Row::from_row(row, cols.clone())?];
                while let Some(row) = rows.next()? {
                  result.push(Row::from_row(row, cols.clone())?);
                }
                return Ok(Some(Rows(result, cols)));
              }

              return Ok(None);
            }
          }
        }

        return Ok(None);
      })
      .await;
  }

  pub fn attach(&self, path: &str, name: &str) -> Result<()> {
    let lock = self.conns.write();
    for conn in &lock.0 {
      conn.execute(&format!("ATTACH DATABASE '{path}' AS {name} "), ())?;
    }
    return Ok(());
  }

  pub async fn list_databases(&self) -> Result<Vec<Database>> {
    return self
      .call(|conn| {
        let mut stmt = conn.prepare("SELECT seq, name FROM pragma_database_list")?;
        let mut rows = stmt.raw_query();

        let mut databases: Vec<Database> = vec![];
        while let Some(row) = rows.next()? {
          databases.push(serde_rusqlite::from_row(row)?)
        }
        return Ok(databases);
      })
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
  pub async fn close(self) -> Result<()> {
    let _ = self.writer.send(Message::Terminate);
    while self.reader.send(Message::Terminate).is_ok() {
      // Continue to close readers while the channel is alive.
    }

    let mut errors = vec![];
    let conns: ConnectionVec = std::mem::take(&mut self.conns.write());
    for conn in conns.0 {
      if let Err((_, err)) = conn.close() {
        errors.push(err);
      };
    }

    if !errors.is_empty() {
      debug!("Closing connection: {errors:?}");
      return Err(Error::Close(errors.swap_remove(0)));
    }

    return Ok(());
  }
}

impl Debug for Connection {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Connection").finish()
  }
}

fn event_loop(id: usize, conns: Arc<RwLock<ConnectionVec>>, receiver: Receiver<Message>) {
  while let Ok(message) = receiver.recv() {
    match message {
      Message::RunConst(f) => {
        let lock = conns.read();
        f(&lock.0[id])
      }
      Message::RunMut(f) => {
        let mut lock = conns.write();
        f(&mut lock.0[0])
      }
      Message::Terminate => {
        return;
      }
    };
  }
}

pub fn extract_row_id(case: &PreUpdateCase) -> Option<i64> {
  return match case {
    PreUpdateCase::Insert(accessor) => Some(accessor.get_new_row_id()),
    PreUpdateCase::Delete(accessor) => Some(accessor.get_old_row_id()),
    PreUpdateCase::Update {
      new_value_accessor: accessor,
      ..
    } => Some(accessor.get_new_row_id()),
    PreUpdateCase::Unknown => None,
  };
}

pub fn extract_record_values(case: &PreUpdateCase) -> Option<Vec<Value>> {
  return Some(match case {
    PreUpdateCase::Insert(accessor) => (0..accessor.get_column_count())
      .map(|idx| -> Value {
        accessor
          .get_new_column_value(idx)
          .map_or(rusqlite::types::Value::Null, |v| v.into())
      })
      .collect(),
    PreUpdateCase::Delete(accessor) => (0..accessor.get_column_count())
      .map(|idx| -> rusqlite::types::Value {
        accessor
          .get_old_column_value(idx)
          .map_or(rusqlite::types::Value::Null, |v| v.into())
      })
      .collect(),
    PreUpdateCase::Update {
      new_value_accessor: accessor,
      ..
    } => (0..accessor.get_column_count())
      .map(|idx| -> rusqlite::types::Value {
        accessor
          .get_new_column_value(idx)
          .map_or(rusqlite::types::Value::Null, |v| v.into())
      })
      .collect(),
    PreUpdateCase::Unknown => {
      return None;
    }
  });
}

pub struct LockGuard<'a> {
  guard: parking_lot::RwLockWriteGuard<'a, ConnectionVec>,
}

impl Deref for LockGuard<'_> {
  type Target = rusqlite::Connection;
  #[inline]
  fn deref(&self) -> &rusqlite::Connection {
    return &self.guard.deref().0[0];
  }
}

impl DerefMut for LockGuard<'_> {
  #[inline]
  fn deref_mut(&mut self) -> &mut rusqlite::Connection {
    return &mut self.guard.deref_mut().0[0];
  }
}

pub struct ArcLockGuard {
  guard: parking_lot::ArcRwLockWriteGuard<parking_lot::RawRwLock, ConnectionVec>,
}

impl Deref for ArcLockGuard {
  type Target = rusqlite::Connection;
  #[inline]
  fn deref(&self) -> &rusqlite::Connection {
    return &self.guard.deref().0[0];
  }
}

impl DerefMut for ArcLockGuard {
  #[inline]
  fn deref_mut(&mut self) -> &mut rusqlite::Connection {
    return &mut self.guard.deref_mut().0[0];
  }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
