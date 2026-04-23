use flume::{Receiver, Sender};
use log::*;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::error::Error;
use crate::params::Params;
pub use crate::sqlite::lock::{ArcLockGuard, LockError, LockGuard};

#[derive(Default)]
pub(super) struct ConnectionVec(pub(super) smallvec::SmallVec<[rusqlite::Connection; 32]>);

// NOTE: We must never access the same connection concurrently even as immutable &Connection, due
// to intrinsic statement cache. We can ensure this by uniquely assigning one connection to each
// thread.
unsafe impl Sync for ConnectionVec {}

enum ReaderMessage {
  RunConst(Box<dyn FnOnce(&rusqlite::Connection) + Send>),
  Terminate,
}

enum WriterMessage {
  RunMut(Box<dyn FnOnce(&mut rusqlite::Connection) + Send>),
}

#[derive(Clone, Default)]
pub struct Options {
  pub busy_timeout: Option<std::time::Duration>,
  pub num_threads: Option<usize>,
}

/// A handle to call functions in background thread.
#[derive(Clone)]
pub(crate) struct Executor {
  reader: Sender<ReaderMessage>,
  writer: Sender<WriterMessage>,
  // NOTE: Is shared across reader and writer worker threads.
  conns: Arc<RwLock<ConnectionVec>>,
}

impl Executor {
  pub fn new<E>(
    builder: impl Fn() -> Result<rusqlite::Connection, E>,
    opt: Options,
  ) -> Result<Self, Error>
  where
    Error: From<E>,
  {
    let Options {
      busy_timeout,
      num_threads,
    } = opt;

    let new_conn = |read_only: bool| -> Result<rusqlite::Connection, Error> {
      let conn = builder()?;
      if read_only {
        conn.pragma_update(None, "query_only", true)?;
      }
      if let Some(busy_timeout) = busy_timeout {
        conn.busy_timeout(busy_timeout)?;
      }
      return Ok(conn);
    };

    let write_conn = new_conn(/* read_only= */ false)?;
    let path = write_conn.path().map(|p| p.to_string());
    let in_memory = path.as_ref().is_none_or(|s| {
      // Returns empty string for in-memory databases.
      return s.is_empty();
    });

    let num_threads: usize = match (in_memory, num_threads.unwrap_or(1)) {
      (true, _) => {
        // We cannot share an in-memory database across threads, they're all independent.
        1
      }
      (false, 0) => {
        warn!("Executor needs at least one thread, falling back to 1.");
        1
      }
      (false, n) => {
        if let Ok(max) = std::thread::available_parallelism()
          && n > max.get()
        {
          warn!(
            "Num threads '{n}' exceeds hardware parallelism: {}",
            max.get()
          );
        }

        n
      }
    };

    assert!(num_threads > 0);

    let num_read_threads = num_threads - 1;
    let conns = Arc::new(RwLock::new(ConnectionVec({
      let mut conns = Vec::with_capacity(num_threads);
      conns.push(write_conn);
      for _ in 0..num_read_threads {
        conns.push(new_conn(/* read_only= */ true)?);
      }
      conns.into()
    })));

    assert_eq!(num_threads, conns.read().0.len());

    let (shared_write_sender, shared_write_receiver) = flume::unbounded::<WriterMessage>();
    let (shared_read_sender, shared_read_receiver) = flume::unbounded::<ReaderMessage>();

    // Spawn writer thread.
    std::thread::Builder::new()
      .name("tb-sqlite-0 (rw)".to_string())
      .spawn({
        let shared_read_receiver = shared_read_receiver.clone();
        let conns = conns.clone();

        move || writer_event_loop(conns, shared_read_receiver, shared_write_receiver)
      })
      .map_err(|err| Error::Other(format!("spawning rw thread failed: {err}").into()))?;

    // Spawn readers threads.
    for index in 0..num_read_threads {
      std::thread::Builder::new()
        .name(format!("tb-sqlite-{index} (ro)"))
        .spawn({
          let shared_read_receiver = shared_read_receiver.clone();
          let conns = conns.clone();

          move || reader_event_loop(index + 1, conns, shared_read_receiver)
        })
        .map_err(|err| Error::Other(format!("spawning ro thread {index} failed: {err}").into()))?;
    }

    debug!(
      "Opened SQLite DB '{}' ({num_threads} threads, in-memory: {in_memory})",
      path.as_deref().unwrap_or("<in-memory>")
    );

    let conn = Self {
      reader: shared_read_sender,
      writer: shared_write_sender,
      conns,
    };

    assert_eq!(num_threads, conn.threads());

    return Ok(conn);
  }

  pub fn threads(&self) -> usize {
    return self.conns.read().0.len();
  }

  #[inline]
  pub fn write_lock(&self) -> Result<LockGuard<'_>, LockError> {
    return Ok(LockGuard {
      guard: self.conns.write(),
    });
  }

  #[inline]
  pub fn try_write_arc_lock_for(
    &self,
    duration: tokio::time::Duration,
  ) -> Result<ArcLockGuard, LockError> {
    return Ok(ArcLockGuard {
      guard: self
        .conns
        .try_write_arc_for(duration)
        .ok_or(LockError::Timeout)?,
    });
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
  pub async fn call_writer<F, R, E>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R, E> + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Error: From<E>,
  {
    // return call_impl(&self.writer, function).await;
    let (sender, receiver) = oneshot::channel::<Result<R, E>>();

    self
      .writer
      .send(WriterMessage::RunMut(Box::new(move |conn| {
        if !sender.is_closed() {
          let _ = sender.send(function(conn));
        }
      })))
      .map_err(|_| Error::ConnectionClosed)?;

    return Ok(receiver.await.map_err(|_| Error::ConnectionClosed)??);
  }

  #[inline]
  pub async fn call_reader<F, R, E>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(&rusqlite::Connection) -> Result<R, E> + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Error: From<E>,
  {
    let (sender, receiver) = oneshot::channel::<Result<R, Error>>();

    self
      .reader
      .send(ReaderMessage::RunConst(Box::new(move |conn| {
        if !sender.is_closed() {
          let _ = sender.send(function(conn).map_err(|err| err.into()));
        }
      })))
      .map_err(|_| Error::ConnectionClosed)?;

    return receiver.await.map_err(|_| Error::ConnectionClosed)?;
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
      .call_writer(move |conn: &mut rusqlite::Connection| {
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

  pub async fn close(self) -> Result<(), Error> {
    while self.reader.send(ReaderMessage::Terminate).is_ok() {
      // Continue to close readers (as well as the reader/writer) while the channel is alive.
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

fn reader_event_loop(
  idx: usize,
  conns: Arc<RwLock<ConnectionVec>>,
  receiver: Receiver<ReaderMessage>,
) {
  while let Ok(message) = receiver.recv() {
    match message {
      ReaderMessage::RunConst(f) => {
        let lock = conns.read();
        f(&lock.0[idx])
      }
      ReaderMessage::Terminate => {
        return;
      }
    };
  }

  debug!("reader thread shut down");
}

fn writer_event_loop(
  conns: Arc<RwLock<ConnectionVec>>,
  reader_receiver: Receiver<ReaderMessage>,
  writer_receiver: Receiver<WriterMessage>,
) {
  while flume::Selector::new()
    .recv(&writer_receiver, |m| {
      let Ok(m) = m else {
        return false;
      };

      return match m {
        WriterMessage::RunMut(f) => {
          let mut lock = conns.write();
          f(&mut lock.0[0]);

          // Continue
          true
        }
      };
    })
    .recv(&reader_receiver, |m| {
      let Ok(m) = m else {
        return false;
      };

      return match m {
        ReaderMessage::Terminate => false,
        ReaderMessage::RunConst(f) => {
          let lock = conns.read();
          f(&lock.0[0]);

          // Continue
          true
        }
      };
    })
    .wait()
  {}

  debug!("writer thread shut down");
}
