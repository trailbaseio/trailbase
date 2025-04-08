use crossbeam_channel::{Receiver, Sender};
use log::*;
use rusqlite::fallible_iterator::FallibleIterator;
use rusqlite::hooks::{Action, PreUpdateCase};
use rusqlite::types::Value;
use std::{
  fmt::{self, Debug},
  sync::Arc,
};
use tokio::sync::oneshot;

use crate::error::Error;
pub use crate::params::Params;
use crate::rows::{columns, Column};
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

/// The result returned on method calls in this crate.
pub type Result<T> = std::result::Result<T, Error>;

type CallFn = Box<dyn FnOnce(&mut rusqlite::Connection) + Send + 'static>;

enum Message {
  Run(CallFn),
  Close(oneshot::Sender<std::result::Result<(), rusqlite::Error>>),
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
}

impl Connection {
  pub fn new<E>(
    builder: impl Fn() -> std::result::Result<rusqlite::Connection, E>,
    opt: Option<Options>,
  ) -> std::result::Result<Self, E> {
    let n_read_threads = {
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
    };

    let spawn = |receiver: Receiver<Message>| -> std::result::Result<String, E> {
      let conn = builder()?;
      let name = conn.path().unwrap_or_default().to_string();
      if let Some(timeout) = opt.as_ref().map(|o| o.busy_timeout) {
        conn.busy_timeout(timeout).expect("busy timeout failed");
      }

      std::thread::spawn(move || event_loop(conn, receiver));

      return Ok(name);
    };

    let (shared_write_sender, shared_write_receiver) = crossbeam_channel::unbounded::<Message>();
    let name = spawn(shared_write_receiver)?;

    let shared_read_sender = if n_read_threads > 0 {
      let (shared_read_sender, shared_read_receiver) = crossbeam_channel::unbounded::<Message>();
      for _ in 0..n_read_threads {
        spawn(shared_read_receiver.clone())?;
      }
      shared_read_sender
    } else {
      shared_write_sender.clone()
    };

    debug!("Opened SQLite DB '{name}' with {n_read_threads} dedicated reader threads");

    return Ok(Self {
      reader: shared_read_sender,
      writer: shared_write_sender,
    });
  }

  /// Open a new connection to an in-memory SQLite database.
  ///
  /// # Failure
  ///
  /// Will return `Err` if the underlying SQLite open call fails.
  pub fn open_in_memory() -> Result<Self> {
    return Self::new(|| Ok(rusqlite::Connection::open_in_memory()?), None);
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
    return call_impl(&self.writer, function).await;
  }

  #[inline]
  pub fn call_and_forget(&self, function: impl FnOnce(&rusqlite::Connection) + Send + 'static) {
    let _ = self
      .writer
      .send(Message::Run(Box::new(move |conn| function(conn))));
  }

  #[inline]
  async fn call_reader<F, R>(&self, function: F) -> Result<R>
  where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R> + Send + 'static,
    R: Send + 'static,
  {
    return call_impl(&self.reader, function).await;
  }

  /// Query SQL statement.
  pub async fn read_query_rows(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Rows> {
    return self
      .call_reader(move |conn: &mut rusqlite::Connection| {
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
      .call_reader(move |conn: &mut rusqlite::Connection| {
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
      .call_reader(move |conn: &mut rusqlite::Connection| {
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

          match p.peek() {
            Err(_) | Ok(None) => {
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
            _ => {}
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
    // Returns true if connection was successfully closed.
    let closer = async |s: &Sender<Message>| -> std::result::Result<bool, rusqlite::Error> {
      let (sender, receiver) = oneshot::channel::<std::result::Result<(), rusqlite::Error>>();
      if let Err(crossbeam_channel::SendError(_)) = s.send(Message::Close(sender)) {
        // If the channel is closed on the other side, it means the connection closed successfully
        // This is a safeguard against calling close on a `Copy` of the connection
        return Ok(false);
      }

      let Ok(result) = receiver.await else {
        // If we get a RecvError at this point, it also means the channel closed in the meantime
        // we can assume the connection is closed
        return Ok(false);
      };

      // Return the error from `conn.close()` if any.
      result?;

      return Ok(true);
    };

    let mut errors = vec![];
    if let Err(err) = closer(&self.writer).await {
      errors.push(Error::Close(err));
    };

    loop {
      match closer(&self.reader).await {
        Ok(closed) => {
          if !closed {
            break;
          }
        }
        Err(err) => {
          errors.push(Error::Close(err));
        }
      }
    }

    if !errors.is_empty() {
      warn!("Closing connection: {errors:?}");
      return Err(errors.swap_remove(0));
    }

    return Ok(());
  }
}

impl Debug for Connection {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Connection").finish()
  }
}

fn event_loop(mut conn: rusqlite::Connection, receiver: Receiver<Message>) {
  const BUG_TEXT: &str = "bug in trailbase-sqlite, please report";

  while let Ok(message) = receiver.recv() {
    match message {
      Message::Run(f) => f(&mut conn),
      Message::Close(ch) => {
        match conn.close() {
          Ok(v) => ch.send(Ok(v)).expect(BUG_TEXT),
          Err((_conn, e)) => ch.send(Err(e)).expect(BUG_TEXT),
        };

        return;
      }
    };
  }
}

#[inline]
async fn call_impl<F, R>(s: &Sender<Message>, function: F) -> Result<R>
where
  F: FnOnce(&mut rusqlite::Connection) -> Result<R> + Send + 'static,
  R: Send + 'static,
{
  let (sender, receiver) = oneshot::channel::<Result<R>>();

  s.send(Message::Run(Box::new(move |conn| {
    let value = function(conn);
    let _ = sender.send(value);
  })))
  .map_err(|_| Error::ConnectionClosed)?;

  receiver.await.map_err(|_| Error::ConnectionClosed)?
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
