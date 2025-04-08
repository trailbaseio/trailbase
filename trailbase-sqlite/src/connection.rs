use crossbeam_channel::{Receiver, Sender};
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
  pub n_threads: usize,
}

impl Default for Options {
  fn default() -> Self {
    return Self {
      busy_timeout: std::time::Duration::from_secs(5),
      n_threads: 1,
    };
  }
}

struct ConnectionState {
  shared: Sender<Message>,
  private: Vec<Sender<Message>>,
}

/// A handle to call functions in background thread.
#[derive(Clone)]
pub struct Connection {
  state: Arc<ConnectionState>,
}

impl Connection {
  pub fn new<E>(
    c: impl Fn() -> std::result::Result<rusqlite::Connection, E>,
    opt: Option<Options>,
  ) -> std::result::Result<Self, E> {
    let (shared_sender, shared_receiver) = crossbeam_channel::unbounded::<Message>();

    let n_threads = opt.as_ref().map_or(1, |o| o.n_threads);
    let private_senders = (0..n_threads)
      .map(|_| {
        let shared_receiver = shared_receiver.clone();
        let (sender, receiver) = crossbeam_channel::unbounded::<Message>();

        let conn = c()?;
        if let Some(timeout) = opt.as_ref().map(|o| o.busy_timeout) {
          conn.busy_timeout(timeout).expect("busy timeout failed");
        }

        std::thread::spawn(move || event_loop(conn, shared_receiver, receiver));

        return Ok::<Sender<Message>, E>(sender);
      })
      .collect::<std::result::Result<Vec<Sender<Message>>, E>>()?;

    return Ok(Self {
      state: Arc::new(ConnectionState {
        shared: shared_sender,
        private: private_senders,
      }),
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
    return Self::call_impl(&self.state.shared, function).await;
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

  pub fn call_and_forget(&self, function: impl FnOnce(&rusqlite::Connection) + Send + 'static) {
    let _ = self
      .state
      .shared
      .send(Message::Run(Box::new(move |conn| function(conn))));
  }

  /// Query SQL statement.
  pub async fn query(&self, sql: &str, params: impl Params + Send + 'static) -> Result<Rows> {
    let sql = sql.to_string();
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(&sql)?;
        params.bind(&mut stmt)?;
        let rows = stmt.raw_query();
        Ok(Rows::from_rows(rows)?)
      })
      .await;
  }

  pub async fn query_row(
    &self,
    sql: &str,
    params: impl Params + Send + 'static,
  ) -> Result<Option<Row>> {
    let sql = sql.to_string();
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(&sql)?;
        params.bind(&mut stmt)?;
        let mut rows = stmt.raw_query();
        if let Some(row) = rows.next()? {
          return Ok(Some(Row::from_row(row, None)?));
        }
        Ok(None)
      })
      .await;
  }

  pub async fn query_value<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: &str,
    params: impl Params + Send + 'static,
  ) -> Result<Option<T>> {
    let sql = sql.to_string();
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(&sql)?;
        params.bind(&mut stmt)?;
        let mut rows = stmt.raw_query();
        if let Some(row) = rows.next()? {
          return Ok(Some(serde_rusqlite::from_row(row)?));
        }
        Ok(None)
      })
      .await;
  }

  pub async fn query_values<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: &str,
    params: impl Params + Send + 'static,
  ) -> Result<Vec<T>> {
    let sql = sql.to_string();
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(&sql)?;
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
  pub async fn execute(&self, sql: &str, params: impl Params + Send + 'static) -> Result<usize> {
    let sql = sql.to_string();
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(&sql)?;
        params.bind(&mut stmt)?;
        Ok(stmt.raw_execute()?)
      })
      .await;
  }

  /// Batch execute SQL statements and return rows of last statement.
  pub async fn execute_batch(&self, sql: &str) -> Result<Option<Rows>> {
    let sql = sql.to_string();
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let batch = rusqlite::Batch::new(conn, &sql);

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
    let arc_hook = hook.map(|h| Arc::new(h));

    for private in &self.state.private {
      let arc_hook = arc_hook.clone();
      Self::call_impl(private, move |conn| {
        conn
          .preupdate_hook(arc_hook.map(|h| {
            move |a: Action, db: &str, table: &str, c: &PreUpdateCase| h(a, db, table, c)
          }));
        return Ok(());
      })
      .await?;
    }

    return Ok(());

    // return self
    //   .call(|conn| {
    //     conn.preupdate_hook(hook);
    //     return Ok(());
    //   })
    //   .await;
  }

  /// Close the database connection.
  ///
  /// This is functionally equivalent to the `Drop` implementation for
  /// `Connection`. It consumes the `Connection`, but on error returns it
  /// to the caller for retry purposes.
  ///
  /// If successful, any following `close` operations performed
  /// on `Connection` copies will succeed immediately.
  ///
  /// On the other hand, any calls to [`Connection::call`] will return a
  /// [`Error::ConnectionClosed`], and any calls to [`Connection::call_unwrap`] will cause a
  /// `panic`.
  ///
  /// # Failure
  ///
  /// Will return `Err` if the underlying SQLite close call fails.
  pub async fn close(self) -> Result<()> {
    let (sender, receiver) = oneshot::channel::<std::result::Result<(), rusqlite::Error>>();

    if let Err(crossbeam_channel::SendError(_)) = self.state.shared.send(Message::Close(sender)) {
      // If the channel is closed on the other side, it means the connection closed successfully
      // This is a safeguard against calling close on a `Copy` of the connection
      return Ok(());
    }

    let Ok(result) = receiver.await else {
      // If we get a RecvError at this point, it also means the channel closed in the meantime
      // we can assume the connection is closed
      return Ok(());
    };

    return result.map_err(|e| Error::Close(self, e));
  }
}

impl Debug for Connection {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Connection").finish()
  }
}

fn event_loop(
  mut conn: rusqlite::Connection,
  shared: Receiver<Message>,
  private: Receiver<Message>,
) {
  const BUG_TEXT: &str = "bug in trailbase-sqlite, please report";

  loop {
    crossbeam_channel::select! {
      recv(shared) -> message => {
        match message {
          Ok(Message::Run(f)) => f(&mut conn),
          Ok(Message::Close(ch)) => {
            match conn.close() {
              Ok(v) => ch.send(Ok(v)).expect(BUG_TEXT),
              Err((_conn, e)) => ch.send(Err(e)).expect(BUG_TEXT),
            };

            return;
          }
          Err(_) => {
            return;
          },
        }
      },
      recv(private) -> message=> {
        match message {
          Ok(Message::Run(f)) => f(&mut conn),
          Ok(Message::Close(ch)) => {
            match conn.close() {
              Ok(v) => ch.send(Ok(v)).expect(BUG_TEXT),
              Err((_conn, e)) => ch.send(Err(e)).expect(BUG_TEXT),
            };

            return;
          }
          Err(_) => {
            return;
          },
        }
      },
    }
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
