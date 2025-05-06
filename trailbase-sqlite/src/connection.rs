use kanal::{Receiver, Sender};
use log::*;
use parking_lot::RwLock;
use rusqlite::fallible_iterator::FallibleIterator;
use rusqlite::hooks::{Action, PreUpdateCase};
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

struct LockedConnections(RwLock<Vec<rusqlite::Connection>>);

// NOTE: We must never access the same connection concurrently even as &Connection, due to
// Statement cache. We can ensure this by uniquely assigning one connection to each thread.
unsafe impl Sync for LockedConnections {}

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
  conns: Arc<LockedConnections>,
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

    let conn = new_conn()?;
    let name = conn.path().and_then(|s| {
      // Returns empty string for in-memory databases.
      if s.is_empty() {
        None
      } else {
        Some(s.to_string())
      }
    });

    let n_read_threads = if name.is_some() {
      let n_read_threads = match opt.as_ref().map_or(0, |o| o.n_read_threads) {
        1 => {
          warn!(
            "Using a single dedicated reader thread won't improve performance, falling back to 0."
          );
          0
        }
        n => n,
      };

      if let Ok(n) = std::thread::available_parallelism() {
        if n_read_threads > n.get() {
          debug!(
            "Using {n_read_threads} exceeding hardware parallelism: {}",
            n.get()
          );
        }
      }

      n_read_threads
    } else {
      // We cannot share an in-memory database across threads, they're all independent.
      0
    };

    let conns = {
      let mut conns = vec![conn];
      for _ in 0..n_read_threads {
        conns.push(new_conn()?);
      }

      Arc::new(LockedConnections(RwLock::new(conns)))
    };

    // Spawn writer.
    let (shared_write_sender, shared_write_receiver) = kanal::unbounded::<Message>();
    let conns_clone = conns.clone();
    std::thread::spawn(move || event_loop(0, conns_clone, shared_write_receiver));

    let shared_read_sender = if n_read_threads > 0 {
      let (shared_read_sender, shared_read_receiver) = kanal::unbounded::<Message>();
      for i in 0..n_read_threads {
        let shared_read_receiver = shared_read_receiver.clone();
        let conns_clone = conns.clone();
        std::thread::spawn(move || event_loop(i, conns_clone, shared_read_receiver));
      }
      shared_read_sender
    } else {
      shared_write_sender.clone()
    };

    debug!(
      "Opened SQLite DB '{name}' with {n_read_threads} dedicated reader threads",
      name = name.as_deref().unwrap_or("<in-memory>")
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
    let conns = Arc::new(LockedConnections(RwLock::new(vec![conn])));
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
      guard: self.conns.0.write(),
    };
  }

  #[inline]
  pub fn try_write_lock_for(&self, duration: tokio::time::Duration) -> Option<LockGuard<'_>> {
    return self
      .conns
      .0
      .try_write_for(duration)
      .map(|guard| LockGuard { guard });
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
  async fn call_reader<F, R>(&self, function: F) -> Result<R>
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
      .read_query_row_f(sql, params, |row| Row::from_row(row, None))
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
        while let Ok(Some(mut stmt)) = p.next() {
          let mut rows = stmt.raw_query();
          let row = rows.next()?;

          match p.peek()? {
            Some(_) => {}
            None => {
              if let Some(row) = row {
                let cols: Arc<Vec<Column>> = Arc::new(columns(row.as_ref()));

                let mut result = vec![Row::from_row(row, Some(cols.clone()))?];
                while let Some(row) = rows.next()? {
                  result.push(Row::from_row(row, Some(cols.clone()))?);
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

  /// Convenience API for (un)setting a new pre-update hook.
  pub async fn add_preupdate_hook(
    &self,
    hook: Option<impl (Fn(Action, &str, &str, &PreUpdateCase)) + Send + Sync + 'static>,
  ) -> Result<()> {
    return self
      .call(move |conn| {
        conn.preupdate_hook(hook);
        return Ok(());
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
    let conns: Vec<_> = std::mem::take(&mut self.conns.0.write());
    for conn in conns {
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

fn event_loop(id: usize, conns: Arc<LockedConnections>, receiver: Receiver<Message>) {
  while let Ok(message) = receiver.recv() {
    match message {
      Message::RunConst(f) => {
        let lock = conns.0.read();
        f(&lock[id])
      }
      Message::RunMut(f) => {
        let mut lock = conns.0.write();
        f(&mut lock[0])
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
  guard: parking_lot::RwLockWriteGuard<'a, Vec<rusqlite::Connection>>,
}

impl Deref for LockGuard<'_> {
  type Target = rusqlite::Connection;
  #[inline]
  fn deref(&self) -> &rusqlite::Connection {
    return &self.guard.deref()[0];
  }
}

impl DerefMut for LockGuard<'_> {
  #[inline]
  fn deref_mut(&mut self) -> &mut rusqlite::Connection {
    return &mut self.guard.deref_mut()[0];
  }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
