use kanal::{Receiver, Sender};
use log::*;
use parking_lot::RwLock;
use rusqlite::fallible_iterator::FallibleIterator;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::oneshot;

use crate::error::Error;
use crate::params::Params;
use crate::rows::{Column, columns};
use crate::rows::{Row, Rows};

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

enum Message {
  RunMut(Box<dyn FnOnce(&mut rusqlite::Connection) + Send>),
  RunConst(Box<dyn FnOnce(&rusqlite::Connection) + Send>),
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
  id: usize,
  reader: Sender<Message>,
  writer: Sender<Message>,
  conns: Arc<RwLock<ConnectionVec>>,
}

impl Connection {
  pub fn new<E>(
    builder: impl Fn() -> Result<rusqlite::Connection, E>,
    opt: Option<Options>,
  ) -> std::result::Result<Self, E> {
    let new_conn = || -> Result<rusqlite::Connection, E> {
      let conn = builder()?;
      if let Some(timeout) = opt.as_ref().map(|o| o.busy_timeout) {
        conn.busy_timeout(timeout).expect("busy timeout failed");
      }
      return Ok(conn);
    };

    let write_conn = new_conn()?;
    let path = write_conn.path().map(|p| p.to_string());
    // Returns empty string for in-memory databases.
    let in_memory = path.as_ref().is_none_or(|s| !s.is_empty());

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
      path.as_deref().unwrap_or("<in-memory>")
    );

    return Ok(Self {
      id: UNIQUE_CONN_ID.fetch_add(1, Ordering::SeqCst),
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
      id: UNIQUE_CONN_ID.fetch_add(1, Ordering::SeqCst),
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
  pub fn open_in_memory() -> Result<Self, Error> {
    return Self::new(|| Ok(rusqlite::Connection::open_in_memory()?), None);
  }

  pub fn id(&self) -> usize {
    return self.id;
  }

  #[inline]
  pub fn write_lock(&self) -> LockGuard<'_> {
    return LockGuard {
      guard: self.conns.write(),
    };
  }

  // #[inline]
  // pub fn try_write_lock_for(&self, duration: tokio::time::Duration) -> Option<LockGuard<'_>> {
  //   return self
  //     .conns
  //     .try_write_for(duration)
  //     .map(|guard| LockGuard { guard });
  // }

  // #[inline]
  // pub fn write_arc_lock(&self) -> ArcLockGuard {
  //   return ArcLockGuard {
  //     guard: self.conns.write_arc(),
  //   };
  // }

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
  pub async fn call<F, R>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R, Error> + Send + 'static,
    R: Send + 'static,
  {
    // return call_impl(&self.writer, function).await;
    let (sender, receiver) = oneshot::channel::<Result<R, Error>>();

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
  pub async fn call_reader<F, R>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(&rusqlite::Connection) -> Result<R, Error> + Send + 'static,
    R: Send + 'static,
  {
    let (sender, receiver) = oneshot::channel::<Result<R, Error>>();

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

  #[inline]
  pub fn call_reader_and_forget(
    &self,
    function: impl FnOnce(&rusqlite::Connection) + Send + 'static,
  ) {
    let _ = self
      .writer
      .send(Message::RunConst(Box::new(move |conn| function(conn))));
  }

  /// Query SQL statement.
  pub async fn read_query_rows(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Rows, Error> {
    return self
      .call_reader(move |conn: &rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;
        assert!(stmt.readonly());

        params.bind(&mut stmt)?;
        let rows = stmt.raw_query();
        Ok(crate::rows::from_rows(rows)?)
      })
      .await;
  }

  pub async fn write_query_rows(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Rows, Error> {
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;

        params.bind(&mut stmt)?;
        let rows = stmt.raw_query();
        Ok(crate::rows::from_rows(rows)?)
      })
      .await;
  }

  pub async fn read_query_row(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<Row>, Error> {
    return self
      .read_query_row_f(sql, params, |row| {
        return crate::rows::from_row(row, Arc::new(columns(row.as_ref())));
      })
      .await;
  }

  #[inline]
  pub async fn query_row_f<T, E: Into<Error>>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    f: impl (FnOnce(&rusqlite::Row<'_>) -> Result<T, E>) + Send + 'static,
  ) -> Result<Option<T>, Error>
  where
    T: Send + 'static,
    Error: From<E>,
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
  ) -> Result<Option<T>, Error>
  where
    T: Send + 'static,
    Error: From<E>,
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
  ) -> Result<Option<T>, Error> {
    return self
      .read_query_row_f(sql, params, |row| {
        serde_rusqlite::from_row(row).map_err(Error::DeserializeValue)
      })
      .await;
  }

  pub async fn write_query_value<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Option<T>, Error> {
    return self
      .query_row_f(sql, params, |row| {
        serde_rusqlite::from_row(row).map_err(Error::DeserializeValue)
      })
      .await;
  }

  pub async fn read_query_values<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Vec<T>, Error> {
    return self
      .call_reader(move |conn: &rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;
        assert!(stmt.readonly());

        params.bind(&mut stmt)?;
        let mut rows = stmt.raw_query();

        let mut values = vec![];
        while let Some(row) = rows.next()? {
          values.push(serde_rusqlite::from_row(row).map_err(Error::DeserializeValue)?);
        }
        return Ok(values);
      })
      .await;
  }

  pub async fn write_query_values<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<Vec<T>, Error> {
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;

        params.bind(&mut stmt)?;
        let mut rows = stmt.raw_query();

        let mut values = vec![];
        while let Some(row) = rows.next()? {
          values.push(serde_rusqlite::from_row(row).map_err(Error::DeserializeValue)?);
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
  ) -> Result<usize, Error> {
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
  pub async fn execute_batch(
    &self,
    sql: impl AsRef<str> + Send + 'static,
  ) -> Result<Option<Rows>, Error> {
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

                let mut result = vec![crate::rows::from_row(row, cols.clone())?];
                while let Some(row) = rows.next()? {
                  result.push(crate::rows::from_row(row, cols.clone())?);
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

  pub fn attach(&self, path: &str, name: &str) -> Result<(), Error> {
    let query = format!("ATTACH DATABASE '{path}' AS {name} ");
    let lock = self.conns.write();
    for conn in &lock.0 {
      conn.execute(&query, ())?;
    }
    return Ok(());
  }

  pub fn detach(&self, name: &str) -> Result<(), Error> {
    let query = format!("DETACH DATABASE {name}");
    let lock = self.conns.write();
    for conn in &lock.0 {
      conn.execute(&query, ())?;
    }
    return Ok(());
  }

  pub async fn list_databases(&self) -> Result<Vec<Database>, Error> {
    return self.call_reader(crate::sqlite::list_databases).await;
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
    let _ = self.writer.send(Message::Terminate);
    while self.reader.send(Message::Terminate).is_ok() {
      // Continue to close readers while the channel is alive.
    }

    let mut errors = vec![];
    let conns: ConnectionVec = std::mem::take(&mut self.conns.write());
    for conn in conns.0 {
      // NOTE: rusqlite's `Connection::close()` returns itself, to allow users to retry
      // failed closes. We on the other, may be left in a partially closed state with multiple
      // connections. Ignorance is bliss.
      if let Err((_self, err)) = conn.close() {
        errors.push(err);
      };
    }

    if !errors.is_empty() {
      warn!("Closing connection: {errors:?}");
      return Err(errors.swap_remove(0).into());
    }

    return Ok(());
  }
}

impl Debug for Connection {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Connection").finish()
  }
}

impl Hash for Connection {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.id.hash(state);
  }
}

impl PartialEq for Connection {
  fn eq(&self, other: &Self) -> bool {
    return self.id == other.id;
  }
}

impl Eq for Connection {}

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

static UNIQUE_CONN_ID: AtomicUsize = AtomicUsize::new(0);
