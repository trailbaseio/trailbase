use kanal::{Receiver, Sender};
use log::*;
use parking_lot::RwLock;
use rusqlite::fallible_iterator::FallibleIterator;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::oneshot;

use crate::error::Error;
use crate::from_sql::{FromSql, FromSqlError};
use crate::params::Params;

#[derive(Default)]
struct ConnectionVec(smallvec::SmallVec<[rusqlite::Connection; 32]>);

// NOTE: We must never access the same connection concurrently even as immutable &Connection, due
// to intrinsic statement cache. We can ensure this by uniquely assigning one connection to each
// thread.
unsafe impl Sync for ConnectionVec {}

enum Message {
  RunMut(Box<dyn FnOnce(&mut rusqlite::Connection) + Send>),
  RunConst(Box<dyn FnOnce(&rusqlite::Connection) + Send>),
  Terminate,
}

#[derive(Clone, Default)]
pub struct Options {
  pub busy_timeout: Option<std::time::Duration>,
  pub n_read_threads: Option<usize>,
}

/// A handle to call functions in background thread.
#[derive(Clone)]
pub(crate) struct ConnectionImpl {
  id: usize,
  reader: Sender<Message>,
  writer: Sender<Message>,
  // NOTE: Is shared across reader and writer worker threads.
  conns: Arc<RwLock<ConnectionVec>>,
}

impl ConnectionImpl {
  pub fn new<E>(
    builder: impl Fn() -> Result<rusqlite::Connection, E>,
    opt: Options,
  ) -> Result<Self, E> {
    let Options {
      busy_timeout,
      n_read_threads,
    } = opt;

    let new_conn = || -> Result<rusqlite::Connection, E> {
      let conn = builder()?;
      if let Some(busy_timeout) = busy_timeout {
        conn
          .busy_timeout(busy_timeout)
          .expect("busy timeout failed");
      }
      return Ok(conn);
    };

    let write_conn = new_conn()?;
    let path = write_conn.path().map(|p| p.to_string());
    let in_memory = path.as_ref().is_none_or(|s| {
      // Returns empty string for in-memory databases.
      return s.is_empty();
    });

    let n_read_threads: i64 = match (in_memory, n_read_threads.unwrap_or(0)) {
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
      conns.into()
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

  pub fn id(&self) -> usize {
    return self.id;
  }

  pub fn threads(&self) -> usize {
    return self.conns.read().0.len();
  }

  #[inline]
  pub fn write_lock(&self) -> LockGuard<'_> {
    return LockGuard {
      guard: self.conns.write(),
    };
  }

  #[inline]
  pub fn try_write_arc_lock_for(&self, duration: tokio::time::Duration) -> Option<ArcLockGuard> {
    return self
      .conns
      .try_write_arc_for(duration)
      .map(|guard| ArcLockGuard { guard });
  }

  #[inline]
  pub(crate) fn map(
    &self,
    f: impl Fn(&rusqlite::Connection) -> Result<(), Error> + Send + 'static,
  ) -> Result<(), Error> {
    let lock = self.conns.write();
    for conn in &lock.0 {
      f(conn)?;
    }
    return Ok(());
  }

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
  pub async fn write_query_rows_f<T>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    f: impl (FnOnce(rusqlite::Rows<'_>) -> Result<T, Error>) + Send + 'static,
  ) -> Result<T, Error>
  where
    T: Send + 'static,
  {
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;

        params.bind(&mut stmt)?;

        return f(stmt.raw_query());
      })
      .await;
  }

  #[inline]
  pub async fn read_query_rows_f<T>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    f: impl (FnOnce(rusqlite::Rows<'_>) -> Result<T, Error>) + Send + 'static,
  ) -> Result<T, Error>
  where
    T: Send + 'static,
  {
    return self
      .call_reader(move |conn: &rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;
        assert!(stmt.readonly());

        params.bind(&mut stmt)?;

        return f(stmt.raw_query());
      })
      .await;
  }

  pub async fn execute(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<usize, Error> {
    return self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut stmt = conn.prepare_cached(sql.as_ref())?;

        params.bind(&mut stmt)?;

        return Ok(stmt.raw_execute()?);
      })
      .await;
  }

  pub async fn execute_batch(&self, sql: impl AsRef<str> + Send + 'static) -> Result<(), Error> {
    self
      .call(move |conn: &mut rusqlite::Connection| {
        let mut batch = rusqlite::Batch::new(conn, sql.as_ref());
        while let Some(mut stmt) = batch.next()? {
          // NOTE: We must use `raw_query` instead of `raw_execute`, otherwise queries
          // returning rows (e.g. SELECT) will return an error. Rusqlite's batch_execute
          // behaves consistently.
          let _row = stmt.raw_query().next()?;
        }
        return Ok(());
      })
      .await?;

    return Ok(());
  }

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

#[inline]
pub(crate) fn map_first<T>(
  mut rows: rusqlite::Rows<'_>,
  f: impl (FnOnce(&rusqlite::Row<'_>) -> Result<T, Error>) + Send + 'static,
) -> Result<Option<T>, Error>
where
  T: Send + 'static,
{
  if let Some(row) = rows.next()? {
    return Ok(Some(f(row)?));
  }
  return Ok(None);
}

#[inline]
pub fn get_value<T: FromSql>(row: &rusqlite::Row<'_>, idx: usize) -> Result<T, Error> {
  let value = row.get_ref(idx)?;

  return FromSql::column_result(value.into()).map_err(|err| {
    use rusqlite::Error as RError;

    return Error::Rusqlite(match err {
      FromSqlError::InvalidType => {
        RError::InvalidColumnType(idx, "<unknown>".into(), value.data_type())
      }
      FromSqlError::OutOfRange(i) => RError::IntegralValueOutOfRange(idx, i),
      FromSqlError::Utf8Error(err) => RError::Utf8Error(idx, err),
      FromSqlError::Other(err) => RError::FromSqlConversionFailure(idx, value.data_type(), err),
      FromSqlError::InvalidBlobSize { .. } => {
        RError::FromSqlConversionFailure(idx, value.data_type(), Box::new(err))
      }
    });
  });
}

static UNIQUE_CONN_ID: AtomicUsize = AtomicUsize::new(0);
