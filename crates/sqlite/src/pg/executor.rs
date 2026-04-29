use flume::{Receiver, Sender};
use log::*;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::error::Error;
use crate::params::Params;
use crate::pg::util::PgStatement;

#[derive(Clone, Default)]
pub struct Options {
  pub num_threads: Option<usize>,
}

enum Message {
  RunMut(Box<dyn FnOnce(&mut postgres::Client) + Send>),
  Terminate,
}

/// A handle to call functions in background thread.
#[allow(unused)]
#[derive(Clone)]
pub(crate) struct Executor {
  sender: Sender<Message>,
  threads: Vec<Sender<Message>>,
}

impl Drop for Executor {
  fn drop(&mut self) {
    let _ = self.close_impl();
  }
}

#[allow(unused)]
impl Executor {
  pub fn new<E>(
    builder: impl Fn() -> Result<postgres::Client, E> + Sync + Send + 'static,
    opt: Options,
  ) -> Result<Self, Error>
  where
    Error: From<E>,
  {
    let Options { num_threads } = opt;

    let conn_builder = Arc::new(move || -> Result<postgres::Client, Error> {
      return Ok(builder()?);
    });

    let num_threads: usize = match num_threads.unwrap_or(1) {
      0 => {
        warn!("Executor needs at least one thread, falling back to 1.");
        1
      }
      n => {
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

    let (shared_sender, shared_receiver) = flume::unbounded::<Message>();
    let threads = (0..num_threads)
      .map(|index| -> Result<Sender<Message>, Error> {
        let shared_receiver = shared_receiver.clone();
        let conn_builder = conn_builder.clone();

        let (s, r) = flume::bounded::<Result<Sender<Message>, Error>>(1);

        std::thread::Builder::new()
          .name(format!("tb-pg-{index}"))
          .spawn(move || -> () {
            let (sender, receiver) = flume::unbounded::<Message>();
            let conn = match conn_builder() {
              Ok(conn) => {
                s.send(Ok(sender)).expect("unreachable");
                conn
              }
              Err(err) => {
                s.send(Err(err)).expect("unreachable");
                return;
              }
            };

            event_loop(index, conn, shared_receiver, receiver);
          })
          .map_err(|err| Error::Other(format!("spawning thread {index} failed: {err}").into()))?;

        return r
          .recv()
          .map_err(|err| Error::Other(format!("recv failed: {err}").into()))?;
      })
      .collect::<Result<Vec<_>, Error>>()?;

    debug!("Opened Postgres DB ({num_threads} threads",);

    return Ok(Self {
      sender: shared_sender,
      threads,
    });
  }

  pub fn threads(&self) -> usize {
    return self.threads.len();
  }

  #[inline]
  pub(crate) async fn map(
    &self,
    f: impl Fn(&mut postgres::Client) -> Result<(), Error> + Sync + Send + 'static,
  ) -> Result<(), Error> {
    let function = Arc::new(f);
    for sender in &self.threads {
      let function = function.clone();
      self
        .sender
        .send(Message::RunMut(Box::new(move |conn| {
          let _ = function(conn);
        })))
        .map_err(|_| Error::ConnectionClosed)?;
    }

    return Ok(());
  }

  #[inline]
  pub async fn call<F, R, E>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(&mut postgres::Client) -> Result<R, E> + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Error: From<E>,
  {
    let (sender, receiver) = oneshot::channel::<Result<R, E>>();

    self
      .sender
      .send(Message::RunMut(Box::new(move |conn| {
        if !sender.is_closed() {
          let _ = sender.send(function(conn));
        }
      })))
      .map_err(|_| Error::ConnectionClosed)?;

    return Ok(receiver.await.map_err(|_| Error::ConnectionClosed)??);
  }

  #[inline]
  pub async fn query_rows_f<T>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    f: impl (FnOnce(postgres::RowIter<'_>) -> Result<T, Error>) + Send + 'static,
  ) -> Result<T, Error>
  where
    T: Send + 'static,
  {
    return self
      .call(move |conn: &mut postgres::Client| {
        let (sql, params) = PgStatement::new(sql.as_ref())?.bind(params)?;
        return f(conn.query_raw(&sql, &params)?);
      })
      .await;
  }

  pub fn close(mut self) -> Result<(), Error> {
    return self.close_impl();
  }

  fn close_impl(&mut self) -> Result<(), Error> {
    while self.sender.send(Message::Terminate).is_ok() {
      // Continue to close readers (as well as the reader/writer) while the channel is alive.
    }
    return Ok(());
  }
}

fn event_loop(
  index: usize,
  mut conn: postgres::Client,
  shared_receiver: Receiver<Message>,
  solo_receiver: Receiver<Message>,
) {
  while let Ok(message) = flume::Selector::new()
    .recv(&shared_receiver, |m| m)
    .recv(&solo_receiver, |m| m)
    .wait()
  {
    match message {
      Message::RunMut(f) => f(&mut conn),
      Message::Terminate => {
        break;
      }
    };
  }

  let r = conn.close();

  debug!("pg worker thread {index} shut down: {r:?}");
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::named_params;

  use postgres::{Client, NoTls, fallible_iterator::FallibleIterator};

  fn build_executor() -> Result<Executor, Error> {
    return Executor::new(
      || {
        return Client::configure()
          .host("localhost")
          .port(5432)
          .user("postgres")
          .password("example")
          .connect(NoTls);
      },
      Options {
        num_threads: Some(2),
      },
    );
  }

  #[tokio::test]
  async fn pg_poc_test() {
    let exec = build_executor().unwrap();
    assert_eq!(2, exec.threads());

    exec
      .call(|client| {
        return client.batch_execute(
          "
            CREATE TABLE IF NOT EXISTS test_table(
              id     SERIAL PRIMARY KEY,
              data   TEXT NOT NULL
            );

            INSERT INTO test_table (data) VALUES ('a'), ('b');
          ",
        );
      })
      .await
      .unwrap();

    let count = exec
      .query_rows_f(
        "SELECT COUNT(*) FROM test_table WHERE data = $1",
        ("a".to_string(),),
        |mut row_iter| -> Result<i64, Error> {
          while let Some(row) = row_iter.next()? {
            return Ok(row.get::<usize, i64>(0));
          }

          return Err(Error::Other("no rows".into()));
        },
      )
      .await
      .unwrap();

    assert!(count > 0);

    exec
      .map(|client| -> Result<(), Error> {
        client.query("SELECT COUNT(*) FROM test_table", &[])?;

        return Ok(());
      })
      .await
      .unwrap();
  }

  #[tokio::test]
  async fn pg_poc_named_parameter_test() {
    let exec = build_executor().unwrap();
    assert_eq!(2, exec.threads());

    exec
      .call(|client| {
        return client.execute(
          "
            CREATE TABLE IF NOT EXISTS test_table_poc_named_params(
              id     SERIAL PRIMARY KEY,
              data   TEXT NOT NULL
            );
          ",
          &[],
        );
      })
      .await
      .unwrap();

    exec
      .query_rows_f(
        "
          INSERT INTO test_table_poc_named_params (data) VALUES (:param);
        ",
        named_params! {":param": "value"},
        |_rows| -> Result<(), Error> {
          return Ok(());
        },
      )
      .await
      .unwrap();
  }
}
