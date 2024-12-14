use crossbeam_channel::{Receiver, Sender};
pub use rusqlite::types::{ToSqlOutput, Value};
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

/// A handle to call functions in background thread.
#[derive(Clone)]
pub struct Connection {
  sender: Sender<Message>,
}

impl Connection {
  pub async fn from_conn(conn: rusqlite::Connection) -> Result<Self> {
    return Ok(start(move || Ok(conn)).await?);
  }

  /// Open a new connection to an in-memory SQLite database.
  ///
  /// # Failure
  ///
  /// Will return `Err` if the underlying SQLite open call fails.
  pub async fn open_in_memory() -> Result<Self> {
    return Ok(start(rusqlite::Connection::open_in_memory).await?);
  }

  /// Call a function in background thread and get the result
  /// asynchronously.
  ///
  /// # Failure
  ///
  /// Will return `Err` if the database connection has been closed.
  pub async fn call<F, R>(&self, function: F) -> Result<R>
  where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R> + 'static + Send,
    R: Send + 'static,
  {
    let (sender, receiver) = oneshot::channel::<Result<R>>();

    self
      .sender
      .send(Message::Run(Box::new(move |conn| {
        let value = function(conn);
        let _ = sender.send(value);
      })))
      .map_err(|_| Error::ConnectionClosed)?;

    receiver.await.map_err(|_| Error::ConnectionClosed)?
  }

  /// Query SQL statement.
  pub async fn query(&self, sql: &str, params: impl Params + Send + 'static) -> Result<Rows> {
    let sql = sql.to_string();
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare(&sql)?;
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
        let mut stmt = conn.prepare(&sql)?;
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
        let mut stmt = conn.prepare(&sql)?;
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
        let mut stmt = conn.prepare(&sql)?;
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
        let mut stmt = conn.prepare(&sql)?;
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
        while let Some(iter) = p.next() {
          let mut stmt = iter?;

          let mut rows = stmt.raw_query();
          let row = rows.next()?;
          if p.peek().is_none() {
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
        return Ok(None);
      })
      .await;
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

    if let Err(crossbeam_channel::SendError(_)) = self.sender.send(Message::Close(sender)) {
      // If the channel is closed on the other side, it means the connection closed successfully
      // This is a safeguard against calling close on a `Copy` of the connection
      return Ok(());
    }

    let result = receiver.await;

    if result.is_err() {
      // If we get a RecvError at this point, it also means the channel closed in the meantime
      // we can assume the connection is closed
      return Ok(());
    }

    result.unwrap().map_err(|e| Error::Close(self, e))
  }
}

impl Debug for Connection {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Connection").finish()
  }
}

async fn start<F>(open: F) -> rusqlite::Result<Connection>
where
  F: FnOnce() -> rusqlite::Result<rusqlite::Connection> + Send + 'static,
{
  let (sender, receiver) = crossbeam_channel::unbounded::<Message>();
  let (result_sender, result_receiver) = oneshot::channel();

  std::thread::spawn(move || {
    let conn = match open() {
      Ok(c) => c,
      Err(e) => {
        let _ = result_sender.send(Err(e));
        return;
      }
    };

    if let Err(_e) = result_sender.send(Ok(())) {
      return;
    }

    event_loop(conn, receiver);
  });

  result_receiver
    .await
    .expect(BUG_TEXT)
    .map(|_| Connection { sender })
}

fn event_loop(mut conn: rusqlite::Connection, receiver: Receiver<Message>) {
  while let Ok(message) = receiver.recv() {
    match message {
      Message::Run(f) => f(&mut conn),
      Message::Close(s) => {
        match conn.close() {
          Ok(v) => s.send(Ok(v)).expect(BUG_TEXT),
          Err((_conn, e)) => s.send(Err(e)).expect(BUG_TEXT),
        };

        return;
      }
    }
  }
}

const BUG_TEXT: &str = "bug in trailbase-sqlite, please report";

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
