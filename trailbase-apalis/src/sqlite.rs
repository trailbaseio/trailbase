use crate::context::SqlContext;
use crate::{calculate_status, Config, SqlError};
use apalis_core::backend::{BackendExpose, Stat, WorkerState};
use apalis_core::codec::json::JsonCodec;
use apalis_core::error::Error;
use apalis_core::layers::{Ack, AckLayer};
use apalis_core::poller::controller::Controller;
use apalis_core::poller::stream::BackendStream;
use apalis_core::poller::Poller;
use apalis_core::request::{Parts, Request, RequestStream, State};
use apalis_core::response::Response;
use apalis_core::storage::Storage;
use apalis_core::task::namespace::Namespace;
use apalis_core::task::task_id::TaskId;
use apalis_core::worker::{Context, Event, Worker, WorkerId};
use apalis_core::{backend::Backend, codec::Codec};
use async_stream::try_stream;
use chrono::{DateTime, Utc};
use futures::{FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use log::error;
use serde::{de::DeserializeOwned, Serialize};
use std::any::type_name;
use std::convert::TryInto;
use std::fmt;
use std::fmt::Debug;
use std::sync::Arc;
use std::{marker::PhantomData, time::Duration};

pub use trailbase_sqlite::Connection;

use crate::from_row::{from_row, SqlRequest};

/// Represents a [Storage] that persists to Sqlite
pub struct SqliteStorage<T, C = JsonCodec<String>> {
  conn: Connection,
  job_type: PhantomData<T>,
  controller: Controller,
  config: Config,
  codec: PhantomData<C>,
}

impl<T, C> fmt::Debug for SqliteStorage<T, C> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("SqliteStorage")
      .field("conn", &self.conn)
      .field("job_type", &"PhantomData<T>")
      .field("controller", &self.controller)
      .field("config", &self.config)
      .field("codec", &std::any::type_name::<C>())
      .finish()
  }
}

impl<T, C> Clone for SqliteStorage<T, C> {
  fn clone(&self) -> Self {
    SqliteStorage {
      conn: self.conn.clone(),
      job_type: PhantomData,
      controller: self.controller.clone(),
      config: self.config.clone(),
      codec: self.codec,
    }
  }
}

mod main {
  trailbase_refinery_macros::embed_migrations!("migrations");
}

impl SqliteStorage<()> {
  /// Perform migrations for storage
  pub async fn setup(conn: &Connection) -> Result<(), trailbase_sqlite::Error> {
    conn
      .execute_batch(
        r#"
      PRAGMA journal_mode = 'WAL';
      PRAGMA temp_store = 2;
      PRAGMA synchronous = NORMAL;
      PRAGMA cache_size = 64000;
    "#,
      )
      .await?;

    let runner = main::migrations::runner();

    let _report = conn
      .call(move |conn| {
        return runner
          .run(conn)
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await?;

    Ok(())
  }
}

impl<T> SqliteStorage<T> {
  /// Create a new instance
  pub fn new(conn: Connection) -> Self {
    Self {
      conn,
      job_type: PhantomData,
      controller: Controller::new(),
      config: Config::new(type_name::<T>()),
      codec: PhantomData,
    }
  }

  /// Create a new instance with a custom config
  pub fn new_with_config(conn: Connection, config: Config) -> Self {
    Self {
      conn,
      job_type: PhantomData,
      controller: Controller::new(),
      config,
      codec: PhantomData,
    }
  }
}
impl<T, C> SqliteStorage<T, C> {
  /// Keeps a storage notified that the worker is still alive manually
  pub async fn keep_alive_at(
    &mut self,
    worker: &Worker<Context>,
    last_seen: i64,
  ) -> Result<(), trailbase_sqlite::Error> {
    const QUERY: &str = "INSERT INTO Workers (id, worker_type, storage_name, layers, last_seen)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (id) DO
                   UPDATE SET last_seen = EXCLUDED.last_seen";

    let worker_type = self.config.namespace.clone();
    let storage_name = std::any::type_name::<Self>();

    self
      .conn
      .execute(
        QUERY,
        trailbase_sqlite::params!(
          worker.id().to_string(),
          worker_type,
          storage_name,
          worker.get_service().to_string(),
          last_seen
        ),
      )
      .await?;

    Ok(())
  }

  /// Expose the connection for other functionality, eg custom migrations
  pub fn conn(&self) -> &Connection {
    &self.conn
  }

  /// Get the config used by the storage
  pub fn get_config(&self) -> &Config {
    &self.config
  }
}

impl<T, C> SqliteStorage<T, C> {
  /// Expose the code used
  pub fn codec(&self) -> &PhantomData<C> {
    &self.codec
  }
}

async fn fetch_next(
  conn: &Connection,
  worker_id: &WorkerId,
  id: String,
  config: &Config,
) -> Result<Option<SqlRequest<String>>, trailbase_sqlite::Error> {
  let now: i64 = Utc::now().timestamp();
  const UPDATE_QUERY: &str = r#"
    UPDATE Jobs SET status = 'Running', lock_by = ?2, lock_at = ?3 WHERE id = ?1 AND job_type = ?4 AND status = 'Pending' AND lock_by IS NULL RETURNING *
  "#;

  let job: Option<SqlRequest<String>> = conn
    .query_row_f(
      UPDATE_QUERY,
      trailbase_sqlite::params!(
        id.to_string(),
        worker_id.to_string(),
        now,
        config.namespace.clone()
      ),
      from_row,
    )
    .await?;

  if job.is_none() {
    let job: Option<SqlRequest<String>> = conn
      .query_row_f(
        "SELECT * FROM Jobs WHERE id = ?1 AND lock_by = ?2 AND job_type = ?3",
        trailbase_sqlite::params!(
          id.to_string(),
          worker_id.to_string(),
          config.namespace.clone()
        ),
        from_row,
      )
      .await?;

    return Ok(job);
  }

  return Ok(job);
}

impl<T, C> SqliteStorage<T, C>
where
  T: DeserializeOwned + Send + Unpin,
  C: Codec<Compact = String>,
{
  fn stream_jobs(
    &self,
    worker: &Worker<Context>,
    interval: Duration,
    buffer_size: usize,
  ) -> impl Stream<Item = Result<Option<Request<T, SqlContext>>, trailbase_sqlite::Error>> {
    const FETCH_QUERY : &str = "SELECT id FROM Jobs
          WHERE (status = 'Pending' OR (status = 'Failed' AND attempts < max_attempts)) AND run_at < ?1 AND job_type = ?2 ORDER BY priority DESC LIMIT ?3";

    let conn = self.conn.clone();
    let worker = worker.clone();
    let config = self.config.clone();
    let namespace = Namespace(self.config.namespace.clone());
    try_stream! {
        loop {
            apalis_core::sleep(interval).await;
            if !worker.is_ready() {
                continue;
            }
            let worker_id = worker.id();
            let job_type = &config.namespace;

            let now: i64 = Utc::now().timestamp();

            let ids: Vec<String> = conn.read_query_values(FETCH_QUERY, trailbase_sqlite::params!(
                now,
                job_type.to_string(),
                buffer_size as i64,
                // i64::try_from(buffer_size).map_err(|e| trailbase_sqlite::Error::Other(e.into()))?,
            ))
                .await?;

            for id in ids {
                let res = fetch_next(&conn, worker_id, id, &config).await?;
                yield match res {
                    None => None::<Request<T, SqlContext>>,
                    Some(job) => {
                        let (req, parts) = job.take_parts();

                        let args = C::decode(req)
                            .map_err(|e| trailbase_sqlite::Error::Other(e.into()))?;

                        let mut req = Request::new_with_parts(args, parts);
                        req.parts.namespace = Some(namespace.clone());
                        Some(req)
                    }
                }
            };
        }
    }
  }
}

impl<T, C> Storage for SqliteStorage<T, C>
where
  T: Serialize + DeserializeOwned + Send + 'static + Unpin + Sync,
  C: Codec<Compact = String> + Send + 'static + Sync,
  C::Error: std::error::Error + Send + Sync + 'static,
{
  type Job = T;

  type Error = trailbase_sqlite::Error;

  type Context = SqlContext;

  type Compact = String;

  async fn push_request(
    &mut self,
    job: Request<Self::Job, SqlContext>,
  ) -> Result<Parts<SqlContext>, Self::Error> {
    const QUERY : &str = "INSERT INTO Jobs VALUES (?1, ?2, ?3, 'Pending', 0, ?4, strftime('%s','now'), NULL, NULL, NULL, NULL, ?5)";
    let (task, parts) = job.take_parts();
    let raw = C::encode(&task).map_err(|e| trailbase_sqlite::Error::Other(e.into()))?;
    let job_type = self.config.namespace.clone();

    self
      .conn
      .execute(
        QUERY,
        trailbase_sqlite::params!(
          raw,
          parts.task_id.to_string(),
          job_type.to_string(),
          parts.context.max_attempts() as i64,
          *parts.context.priority() as i64,
        ),
      )
      .await?;

    Ok(parts)
  }

  async fn push_raw_request(
    &mut self,
    job: Request<Self::Compact, SqlContext>,
  ) -> Result<Parts<SqlContext>, Self::Error> {
    const QUERY :&str  = "INSERT INTO Jobs VALUES (?1, ?2, ?3, 'Pending', 0, ?4, strftime('%s','now'), NULL, NULL, NULL, NULL, ?5)";
    let (task, parts) = job.take_parts();
    let raw = C::encode(&task).map_err(|e| trailbase_sqlite::Error::Other(e.into()))?;

    let job_type = self.config.namespace.clone();
    self
      .conn
      .execute(
        QUERY,
        trailbase_sqlite::params!(
          raw,
          parts.task_id.to_string(),
          job_type.to_string(),
          parts.context.max_attempts() as i64,
          *parts.context.priority() as i64,
        ),
      )
      .await?;
    Ok(parts)
  }

  async fn schedule_request(
    &mut self,
    req: Request<Self::Job, SqlContext>,
    on: i64,
  ) -> Result<Parts<SqlContext>, Self::Error> {
    const QUERY: &str =
      "INSERT INTO Jobs VALUES (?1, ?2, ?3, 'Pending', 0, ?4, ?5, NULL, NULL, NULL, NULL, ?6)";
    let id = &req.parts.task_id;
    let job = C::encode(&req.args).map_err(|e| trailbase_sqlite::Error::Other(e.into()))?;

    let job_type = self.config.namespace.clone();
    self
      .conn
      .execute(
        QUERY,
        trailbase_sqlite::params!(
          job,
          id.to_string(),
          job_type,
          req.parts.context.max_attempts() as i64,
          *req.parts.context.priority() as i64,
          on,
        ),
      )
      .await?;

    Ok(req.parts)
  }

  async fn fetch_by_id(
    &mut self,
    job_id: &TaskId,
  ) -> Result<Option<Request<Self::Job, SqlContext>>, Self::Error> {
    const FETCH_QUERY: &str = "SELECT * FROM Jobs WHERE id = ?1";

    let res: Option<SqlRequest<String>> = self
      .conn
      .query_row_f(FETCH_QUERY, (job_id.to_string(),), from_row)
      .await?;

    match res {
      None => Ok(None),
      Some(job) => Ok(Some({
        let (req, parts) = job.take_parts();
        let args = C::decode(req).map_err(|e| trailbase_sqlite::Error::Other(e.into()))?;

        let mut req: Request<T, SqlContext> = Request::new_with_parts(args, parts);
        req.parts.namespace = Some(Namespace(self.config.namespace.clone()));
        req
      })),
    }
  }

  async fn len(&mut self) -> Result<i64, Self::Error> {
    const QUERY : &str = "SELECT COUNT(*) AS count FROM Jobs WHERE (status = 'Pending' OR (status = 'Failed' AND attempts < max_attempts))";

    let count: Option<i64> = self
      .conn
      .read_query_row_f(QUERY, (), |row| row.get(0))
      .await?;

    return Ok(count.unwrap_or_default());
  }

  async fn reschedule(
    &mut self,
    job: Request<T, SqlContext>,
    wait: Duration,
  ) -> Result<(), Self::Error> {
    let task_id = job.parts.task_id;

    const QUERY : &str =
                "UPDATE Jobs SET status = 'Failed', done_at = NULL, lock_by = NULL, lock_at = NULL, run_at = ?2 WHERE id = ?1";

    let wait: i64 = wait.as_secs() as i64;
    let now: i64 = Utc::now().timestamp();
    let wait_until = now + wait;

    self
      .conn
      .execute(
        QUERY,
        trailbase_sqlite::params!(task_id.to_string(), wait_until,),
      )
      .await?;

    Ok(())
  }

  async fn update(&mut self, job: Request<Self::Job, SqlContext>) -> Result<(), Self::Error> {
    let ctx = job.parts.context;
    let status = ctx.status().to_string();
    let attempts = job.parts.attempt;
    let done_at = *ctx.done_at();
    let lock_by = ctx.lock_by().clone();
    let lock_at = *ctx.lock_at();
    let last_error = ctx.last_error().clone();
    let priority = *ctx.priority();
    let job_id = job.parts.task_id;

    const QUERY : &str =
                "UPDATE Jobs SET status = ?1, attempts = ?2, done_at = ?3, lock_by = ?4, lock_at = ?5, last_error = ?6, priority = ?7 WHERE id = ?8";

    self
      .conn
      .execute(
        QUERY,
        trailbase_sqlite::params!(
          status.to_owned(),
          attempts.current() as i64,
          done_at,
          lock_by.map(|w| w.name().to_string()),
          lock_at,
          last_error,
          priority as i64,
          job_id.to_string()
        ),
      )
      .await?;

    Ok(())
  }

  async fn is_empty(&mut self) -> Result<bool, Self::Error> {
    self.len().map_ok(|c| c == 0).await
  }

  async fn vacuum(&mut self) -> Result<usize, trailbase_sqlite::Error> {
    const QUERY: &str = "Delete from Jobs where status='Done'";

    let rows_affected = self.conn.execute(QUERY, ()).await?;

    Ok(rows_affected)
  }
}

impl<T, C> SqliteStorage<T, C> {
  /// Puts the job instantly back into the queue
  /// Another Worker may consume
  pub async fn retry(
    &mut self,
    worker_id: &WorkerId,
    job_id: &TaskId,
  ) -> Result<(), trailbase_sqlite::Error> {
    const QUERY : &str =
                "UPDATE Jobs SET status = 'Pending', done_at = NULL, lock_by = NULL WHERE id = ?1 AND lock_by = ?2";

    self
      .conn
      .execute(
        QUERY,
        trailbase_sqlite::params!(job_id.to_string(), worker_id.to_string()),
      )
      .await?;
    Ok(())
  }

  /// Kill a job
  pub async fn kill(
    &mut self,
    worker_id: &WorkerId,
    job_id: &TaskId,
  ) -> Result<(), trailbase_sqlite::Error> {
    const QUERY : &str =
                "UPDATE Jobs SET status = 'Killed', done_at = strftime('%s','now') WHERE id = ?1 AND lock_by = ?2";

    self
      .conn
      .execute(
        QUERY,
        trailbase_sqlite::params!(job_id.to_string(), worker_id.to_string(),),
      )
      .await?;

    Ok(())
  }

  /// Add jobs that workers have disappeared to the queue
  pub async fn reenqueue_orphaned(
    &self,
    count: i32,
    dead_since: DateTime<Utc>,
  ) -> Result<(), trailbase_sqlite::Error> {
    const QUERY: &str = r#"UPDATE Jobs
                            SET status = "Pending", done_at = NULL, lock_by = NULL, lock_at = NULL, attempts = attempts + 1, last_error ="Job was abandoned"
                            WHERE id in
                                (SELECT Jobs.id from Jobs INNER join Workers ON lock_by = Workers.id
                                    WHERE status= "Running" AND workers.last_seen < ?1
                                    AND Workers.worker_type = ?2 ORDER BY lock_at ASC LIMIT ?3);"#;

    let job_type = self.config.namespace.clone();

    self
      .conn
      .execute(
        QUERY,
        trailbase_sqlite::params!(dead_since.timestamp(), job_type, count as i64,),
      )
      .await?;
    Ok(())
  }
}

/// Errors that can occur while polling an SQLite database.
#[derive(thiserror::Error, Debug)]
pub enum SqlitePollError {
  /// Error during a keep-alive heartbeat.
  #[error("Encountered an error during KeepAlive heartbeat: `{0}`")]
  KeepAliveError(trailbase_sqlite::Error),

  /// Error during re-enqueuing orphaned tasks.
  #[error("Encountered an error during ReenqueueOrphaned heartbeat: `{0}`")]
  ReenqueueOrphanedError(trailbase_sqlite::Error),
}

impl<T, C> Backend<Request<T, SqlContext>> for SqliteStorage<T, C>
where
  C: Codec<Compact = String> + Send + 'static + Sync,
  C::Error: std::error::Error + 'static + Send + Sync,
  T: Serialize + DeserializeOwned + Sync + Send + Unpin + 'static,
{
  type Stream = BackendStream<RequestStream<Request<T, SqlContext>>>;
  type Layer = AckLayer<SqliteStorage<T, C>, T, SqlContext, C>;

  type Codec = JsonCodec<String>;

  fn poll(mut self, worker: &Worker<Context>) -> Poller<Self::Stream, Self::Layer> {
    let layer = AckLayer::new(self.clone());
    let config = self.config.clone();
    let controller = self.controller.clone();
    let stream = self
      .stream_jobs(worker, config.poll_interval, config.buffer_size)
      .map_err(|e| Error::SourceError(Arc::new(Box::new(e))));
    let stream = BackendStream::new(stream.boxed(), controller);
    let requeue_storage = self.clone();
    let w = worker.clone();
    let heartbeat = async move {
      // Lets reenqueue any jobs that belonged to this worker in case of a death
      if let Err(e) = self
        .reenqueue_orphaned((config.buffer_size * 10) as i32, Utc::now())
        .await
      {
        w.emit(Event::Error(Box::new(
          SqlitePollError::ReenqueueOrphanedError(e),
        )));
      }
      loop {
        let now: i64 = Utc::now().timestamp();
        if let Err(e) = self.keep_alive_at(&w, now).await {
          w.emit(Event::Error(Box::new(SqlitePollError::KeepAliveError(e))));
        }
        apalis_core::sleep(Duration::from_secs(30)).await;
      }
    }
    .boxed();

    let w = worker.clone();
    let reenqueue_beat = async move {
      loop {
        let dead_since =
          Utc::now() - chrono::Duration::from_std(config.reenqueue_orphaned_after).unwrap();
        if let Err(e) = requeue_storage
          .reenqueue_orphaned(
            config
              .buffer_size
              .try_into()
              .expect("could not convert usize to i32"),
            dead_since,
          )
          .await
        {
          w.emit(Event::Error(Box::new(
            SqlitePollError::ReenqueueOrphanedError(e),
          )));
        }
        apalis_core::sleep(config.poll_interval).await;
      }
    };
    Poller::new_with_layer(
      stream,
      async {
        futures::join!(heartbeat, reenqueue_beat);
      },
      layer,
    )
  }
}

impl<T: Sync + Send, C: Send, Res: Serialize + Sync> Ack<T, Res, C> for SqliteStorage<T, C> {
  type Context = SqlContext;
  type AckError = trailbase_sqlite::Error;

  async fn ack(
    &mut self,
    ctx: &Self::Context,
    res: &Response<Res>,
  ) -> Result<(), trailbase_sqlite::Error> {
    let conn = self.conn.clone();
    const QUERY : &str =
                "UPDATE Jobs SET status = ?4, attempts = ?5, done_at = strftime('%s','now'), last_error = ?3 WHERE id = ?1 AND lock_by = ?2";
    let result = serde_json::to_string(&res.inner.as_ref().map_err(|r| r.to_string()))
      .map_err(|e| trailbase_sqlite::Error::Other(e.into()))?;

    conn
      .execute(
        QUERY,
        trailbase_sqlite::params!(
          res.task_id.to_string(),
          ctx
            .lock_by()
            .as_ref()
            .expect("Task is not locked")
            .to_string(),
          result,
          calculate_status(ctx, res).to_string(),
          res.attempt.current() as i64
        ),
      )
      .await?;
    Ok(())
  }
}

impl<J: 'static + Serialize + DeserializeOwned + Unpin + Send + Sync> BackendExpose<J>
  for SqliteStorage<J, JsonCodec<String>>
{
  type Request = Request<J, Parts<SqlContext>>;
  type Error = SqlError;

  async fn stats(&self) -> Result<Stat, Self::Error> {
    const FETCH_QUERY: &str = "SELECT
                            COUNT(1) FILTER (WHERE status = 'Pending') AS pending,
                            COUNT(1) FILTER (WHERE status = 'Running') AS running,
                            COUNT(1) FILTER (WHERE status = 'Done') AS done,
                            COUNT(1) FILTER (WHERE status = 'Failed') AS failed,
                            COUNT(1) FILTER (WHERE status = 'Killed') AS killed
                        FROM Jobs WHERE job_type = ?";

    let res: Option<(i64, i64, i64, i64, i64)> = self
      .conn
      .query_row_f(
        FETCH_QUERY,
        (self.get_config().namespace().to_string(),),
        |row| -> Result<_, trailbase_sqlite::Error> {
          Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
          ))
        },
      )
      .await?;

    let Some(res) = res else {
      return Err(trailbase_sqlite::Error::Other("missing".into()).into());
    };

    Ok(Stat {
      pending: res.0.try_into()?,
      running: res.1.try_into()?,
      dead: res.4.try_into()?,
      failed: res.3.try_into()?,
      success: res.2.try_into()?,
    })
  }

  async fn list_jobs(&self, status: &State, page: i32) -> Result<Vec<Self::Request>, Self::Error> {
    const FETCH_QUERY : &str = "SELECT * FROM Jobs WHERE status = ? AND job_type = ? ORDER BY done_at DESC, run_at DESC LIMIT 10 OFFSET ?";

    let status = status.to_string();
    let res: Vec<SqlRequest<String>> = self
      .conn
      .read_query_values(
        FETCH_QUERY,
        trailbase_sqlite::params!(
          status,
          self.get_config().namespace().to_string(),
          ((page - 1) * 10).to_string(),
        ),
      )
      .await?;
    Ok(
      res
        .into_iter()
        .map(|j| {
          let (req, ctx) = j.take_parts();
          let req = JsonCodec::<String>::decode(req).unwrap();
          Request::new_with_ctx(req, ctx)
        })
        .collect(),
    )
  }

  async fn list_workers(&self) -> Result<Vec<Worker<WorkerState>>, Self::Error> {
    const FETCH_QUERY : &str =
            "SELECT id, layers, last_seen FROM Workers WHERE worker_type = ? ORDER BY last_seen DESC LIMIT 20 OFFSET ?";

    let res: Vec<(String, String, i64)> = self
      .conn
      .read_query_values(
        FETCH_QUERY,
        trailbase_sqlite::params!(self.get_config().namespace().to_string(), 0,),
      )
      .await?;

    Ok(
      res
        .into_iter()
        .map(|w| Worker::new(WorkerId::new(w.0), WorkerState::new::<Self>(w.1)))
        .collect(),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::sql_storage_tests;

  use apalis_core::generic_storage_test;
  use apalis_core::request::State;
  use apalis_core::test_utils::apalis_test_service_fn;
  use apalis_core::test_utils::TestWrapper;
  use futures::StreamExt;
  use std::io;

  mod email_service {
    use apalis_core::error::Error;
    use email_address::EmailAddress;
    use serde::{Deserialize, Serialize};
    use std::str::FromStr;
    use std::sync::Arc;

    #[derive(Debug, Deserialize, Serialize, Clone)]
    pub(super) struct Email {
      pub to: String,
      pub subject: String,
      pub text: String,
    }

    pub(super) async fn send_email(job: Email) -> Result<(), Error> {
      let validation = EmailAddress::from_str(&job.to);
      match validation {
        Ok(email) => {
          log::info!("Attempting to send email to {}", email.as_str());
          Ok(())
        }
        Err(email_address::Error::InvalidCharacter) => {
          log::error!("Killed send email job. Invalid character {}", job.to);
          Err(Error::Abort(Arc::new(Box::new(
            email_address::Error::InvalidCharacter,
          ))))
        }
        Err(e) => Err(Error::Failed(Arc::new(Box::new(e)))),
      }
    }

    pub(super) fn example_good_email() -> Email {
      Email {
        subject: "Test Subject".to_string(),
        to: "example@gmail.com".to_string(),
        text: "Some Text".to_string(),
      }
    }

    pub(super) fn example_killed_email() -> Email {
      Email {
        subject: "Test Subject".to_string(),
        to: "example@©.com".to_string(), // killed because it has © which is invalid
        text: "Some Text".to_string(),
      }
    }

    pub(super) fn example_retry_able_email() -> Email {
      Email {
        subject: "Test Subject".to_string(),
        to: "example".to_string(),
        text: "Some Text".to_string(),
      }
    }
  }

  use email_service::Email;

  generic_storage_test!(setup);
  sql_storage_tests!(setup::<Email>, SqliteStorage<Email>, Email);

  /// migrate DB and return a storage instance.
  async fn setup<T: Serialize + DeserializeOwned>() -> SqliteStorage<T> {
    // Because connections cannot be shared across async runtime
    // (different runtimes are created for each test),
    // we don't share the storage and tests must be run sequentially.
    let conn = Connection::open_in_memory().unwrap();
    SqliteStorage::setup(&conn)
      .await
      .expect("failed to migrate DB");
    let config = Config::new("apalis::test");
    let storage = SqliteStorage::<T>::new_with_config(conn, config);

    storage
  }

  #[tokio::test]
  async fn test_inmemory_sqlite_worker() {
    let mut sqlite = setup().await;
    sqlite
      .push(Email {
        subject: "Test Subject".to_string(),
        to: "example@sqlite".to_string(),
        text: "Some Text".to_string(),
      })
      .await
      .expect("Unable to push job");
    let len = sqlite.len().await.expect("Could not fetch the jobs count");
    assert_eq!(len, 1);
  }

  async fn consume_one(
    storage: &mut SqliteStorage<Email>,
    worker: &Worker<Context>,
  ) -> Request<Email, SqlContext> {
    let mut stream = storage
      .stream_jobs(worker, std::time::Duration::from_secs(10), 1)
      .boxed();

    return stream
      .next()
      .await
      .expect("stream is empty")
      .expect("failed to poll job")
      .expect("no job is pending");
  }

  async fn register_worker_at(
    storage: &mut SqliteStorage<Email>,
    last_seen: i64,
  ) -> Worker<Context> {
    let worker_id = WorkerId::new("test-worker");

    let worker = Worker::new(worker_id, Default::default());
    storage
      .keep_alive_at(&worker, last_seen)
      .await
      .expect("failed to register worker");

    worker.start();
    worker
  }

  async fn register_worker(storage: &mut SqliteStorage<Email>) -> Worker<Context> {
    register_worker_at(storage, Utc::now().timestamp()).await
  }

  async fn push_email(storage: &mut SqliteStorage<Email>, email: Email) {
    storage.push(email).await.expect("failed to push a job");
  }

  async fn get_job(
    storage: &mut SqliteStorage<Email>,
    job_id: &TaskId,
  ) -> Request<Email, SqlContext> {
    storage
      .fetch_by_id(job_id)
      .await
      .expect("failed to fetch job by id")
      .expect("no job found by id")
  }

  #[tokio::test]
  async fn test_consume_last_pushed_job() {
    let mut storage = setup().await;
    let worker = register_worker(&mut storage).await;

    push_email(&mut storage, email_service::example_good_email()).await;
    let len = storage.len().await.expect("Could not fetch the jobs count");
    assert_eq!(len, 1);

    let job = consume_one(&mut storage, &worker).await;
    let ctx = job.parts.context;
    assert_eq!(*ctx.status(), State::Running);
    assert_eq!(*ctx.lock_by(), Some(worker.id().clone()));
    assert!(ctx.lock_at().is_some());
  }

  #[tokio::test]
  async fn test_acknowledge_job() {
    let mut storage = setup().await;
    let worker = register_worker(&mut storage).await;

    push_email(&mut storage, email_service::example_good_email()).await;
    let job = consume_one(&mut storage, &worker).await;
    let job_id = &job.parts.task_id;
    let ctx = &job.parts.context;
    let res = 1usize;
    storage
      .ack(
        ctx,
        &Response::success(res, job_id.clone(), job.parts.attempt.clone()),
      )
      .await
      .expect("failed to acknowledge the job");

    let job = get_job(&mut storage, job_id).await;
    let ctx = job.parts.context;
    assert_eq!(*ctx.status(), State::Done);
    assert!(ctx.done_at().is_some());
  }

  #[tokio::test]
  async fn test_kill_job() {
    let mut storage = setup().await;

    push_email(&mut storage, email_service::example_good_email()).await;

    let _rows = storage
      .conn()
      .read_query_rows("SELECT * FROM Jobs;", ())
      .await
      .unwrap();

    let worker = register_worker(&mut storage).await;

    let job = consume_one(&mut storage, &worker).await;
    let job_id = &job.parts.task_id;

    storage
      .kill(&worker.id(), job_id)
      .await
      .expect("failed to kill job");

    let job = get_job(&mut storage, job_id).await;
    let ctx = job.parts.context;
    assert_eq!(*ctx.status(), State::Killed);
    assert!(ctx.done_at().is_some());
  }

  #[tokio::test]
  async fn test_heartbeat_renqueueorphaned_pulse_last_seen_6min() {
    let mut storage = setup().await;

    push_email(&mut storage, email_service::example_good_email()).await;

    let six_minutes_ago = Utc::now() - Duration::from_secs(6 * 60);

    let five_minutes_ago = Utc::now() - Duration::from_secs(5 * 60);
    let worker = register_worker_at(&mut storage, six_minutes_ago.timestamp()).await;

    let job = consume_one(&mut storage, &worker).await;
    let job_id = &job.parts.task_id;
    storage
      .reenqueue_orphaned(1, five_minutes_ago)
      .await
      .expect("failed to heartbeat");
    let job = get_job(&mut storage, job_id).await;
    let ctx = &job.parts.context;
    assert_eq!(*ctx.status(), State::Pending);
    assert!(ctx.done_at().is_none());
    assert!(ctx.lock_by().is_none());
    assert!(ctx.lock_at().is_none());
    assert_eq!(*ctx.last_error(), Some("Job was abandoned".to_owned()));
    assert_eq!(job.parts.attempt.current(), 1);

    let job = consume_one(&mut storage, &worker).await;
    let ctx = &job.parts.context;
    // Simulate worker
    job.parts.attempt.increment();
    storage
      .ack(
        ctx,
        &Response::new(Ok("success".to_owned()), job_id.clone(), job.parts.attempt),
      )
      .await
      .unwrap();
    //end simulate worker

    let job = get_job(&mut storage, &job_id).await;
    let ctx = &job.parts.context;
    assert_eq!(*ctx.status(), State::Done);
    assert_eq!(*ctx.lock_by(), Some(worker.id().clone()));
    assert!(ctx.lock_at().is_some());
    assert_eq!(*ctx.last_error(), Some("{\"Ok\":\"success\"}".to_owned()));
    assert_eq!(job.parts.attempt.current(), 2);
  }

  #[tokio::test]
  async fn test_heartbeat_renqueueorphaned_pulse_last_seen_4min() {
    let mut storage = setup().await;

    push_email(&mut storage, email_service::example_good_email()).await;

    let six_minutes_ago = Utc::now() - Duration::from_secs(6 * 60);
    let four_minutes_ago = Utc::now() - Duration::from_secs(4 * 60);
    let worker = register_worker_at(&mut storage, four_minutes_ago.timestamp()).await;

    let job = consume_one(&mut storage, &worker).await;
    let job_id = job.parts.task_id;
    storage
      .reenqueue_orphaned(1, six_minutes_ago)
      .await
      .expect("failed to heartbeat");

    let job = get_job(&mut storage, &job_id).await;
    let ctx = &job.parts.context;

    // Simulate worker
    job.parts.attempt.increment();
    storage
      .ack(
        ctx,
        &Response::new(Ok("success".to_owned()), job_id.clone(), job.parts.attempt),
      )
      .await
      .unwrap();
    //end simulate worker

    let job = get_job(&mut storage, &job_id).await;
    let ctx = &job.parts.context;
    assert_eq!(*ctx.status(), State::Done);
    assert_eq!(*ctx.lock_by(), Some(worker.id().clone()));
    assert!(ctx.lock_at().is_some());
    assert_eq!(*ctx.last_error(), Some("{\"Ok\":\"success\"}".to_owned()));
    assert_eq!(job.parts.attempt.current(), 1);
  }
}
