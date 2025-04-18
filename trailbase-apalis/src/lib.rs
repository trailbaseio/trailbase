#![allow(clippy::needless_return)]
#![warn(missing_debug_implementations, missing_docs, unreachable_pub)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! # trailbase-apalis
//!
//! apalis offers Sqlite storages for its workers.
//! See relevant modules for examples

use std::{num::TryFromIntError, time::Duration};

use apalis_core::{error::Error, request::State, response::Response};

/// The context of the sql job
pub mod context;

/// Sqlite Storage for apalis.
/// Uses a transaction and min(rowid)
#[cfg_attr(docsrs, doc(cfg(feature = "sqlite")))]
pub mod sqlite;

use context::SqlContext;

/// Config for sql storages
#[derive(Debug, Clone)]
pub struct Config {
  keep_alive: Duration,
  buffer_size: usize,
  poll_interval: Duration,
  reenqueue_orphaned_after: Duration,
  namespace: String,
}

/// A general sql error
#[derive(Debug, thiserror::Error)]
pub enum SqlError {
  /// TrailBase error.
  #[error("SQL Error: {0}")]
  TB(#[from] trailbase_sqlite::Error),
  /// Conversion error
  #[error("TryFromIntError: {0}")]
  TryFromInt(#[from] TryFromIntError),
}

impl Default for Config {
  fn default() -> Self {
    Self {
      keep_alive: Duration::from_secs(30),
      buffer_size: 10,
      poll_interval: Duration::from_millis(100),
      reenqueue_orphaned_after: Duration::from_secs(300), // 5 minutes
      namespace: String::from("apalis::sql"),
    }
  }
}

impl Config {
  /// Create a new config with a jobs namespace
  pub fn new(namespace: &str) -> Self {
    Config::default().set_namespace(namespace)
  }

  /// Interval between database poll queries
  ///
  /// Defaults to 100ms
  pub fn set_poll_interval(mut self, interval: Duration) -> Self {
    self.poll_interval = interval;
    self
  }

  /// Interval between worker keep-alive database updates
  ///
  /// Defaults to 30s
  pub fn set_keep_alive(mut self, keep_alive: Duration) -> Self {
    self.keep_alive = keep_alive;
    self
  }

  /// Buffer size to use when querying for jobs
  ///
  /// Defaults to 10
  pub fn set_buffer_size(mut self, buffer_size: usize) -> Self {
    self.buffer_size = buffer_size;
    self
  }

  /// Set the namespace to consume and push jobs to
  ///
  /// Defaults to "apalis::sql"
  pub fn set_namespace(mut self, namespace: &str) -> Self {
    self.namespace = namespace.to_string();
    self
  }

  /// Gets a reference to the keep_alive duration.
  pub fn keep_alive(&self) -> &Duration {
    &self.keep_alive
  }

  /// Gets a mutable reference to the keep_alive duration.
  pub fn keep_alive_mut(&mut self) -> &mut Duration {
    &mut self.keep_alive
  }

  /// Gets the buffer size.
  pub fn buffer_size(&self) -> usize {
    self.buffer_size
  }

  /// Gets a reference to the poll_interval duration.
  pub fn poll_interval(&self) -> &Duration {
    &self.poll_interval
  }

  /// Gets a mutable reference to the poll_interval duration.
  pub fn poll_interval_mut(&mut self) -> &mut Duration {
    &mut self.poll_interval
  }

  /// Gets a reference to the namespace.
  pub fn namespace(&self) -> &String {
    &self.namespace
  }

  /// Gets a mutable reference to the namespace.
  pub fn namespace_mut(&mut self) -> &mut String {
    &mut self.namespace
  }

  /// Gets the reenqueue_orphaned_after duration.
  pub fn reenqueue_orphaned_after(&self) -> Duration {
    self.reenqueue_orphaned_after
  }

  /// Gets a mutable reference to the reenqueue_orphaned_after.
  pub fn reenqueue_orphaned_after_mut(&mut self) -> &mut Duration {
    &mut self.reenqueue_orphaned_after
  }

  /// Occasionally some workers die, or abandon jobs because of panics.
  /// This is the time a task takes before its back to the queue
  ///
  /// Defaults to 5 minutes
  pub fn set_reenqueue_orphaned_after(mut self, after: Duration) -> Self {
    self.reenqueue_orphaned_after = after;
    self
  }
}

/// Calculates the status from a result
pub fn calculate_status<Res>(ctx: &SqlContext, res: &Response<Res>) -> State {
  match &res.inner {
    Ok(_) => State::Done,
    Err(e) => match &e {
      Error::Abort(_) => State::Killed,
      Error::Failed(_) if ctx.max_attempts() as usize <= res.attempt.current() => State::Killed,
      _ => State::Failed,
    },
  }
}

/// Standard checks for any sql backend
#[macro_export]
macro_rules! sql_storage_tests {
  ($setup:path, $storage_type:ty, $job_type:ty) => {
    type WrappedStorage = TestWrapper<$storage_type, Request<$job_type, SqlContext>, ()>;

    async fn setup_test_wrapper() -> WrappedStorage {
      let (mut t, poller) = TestWrapper::new_with_service(
        $setup().await,
        apalis_core::service_fn::service_fn(email_service::send_email),
      );
      tokio::spawn(poller);
      t.vacuum().await.unwrap();
      t
    }

    async fn push_email_priority(
      storage: &mut WrappedStorage,
      email: Email,
      priority: i32,
    ) -> TaskId {
      let mut ctx = SqlContext::new();
      ctx.set_priority(priority);
      storage
        .push_request(Request::new_with_ctx(email, ctx))
        .await
        .expect("failed to push a job")
        .task_id
    }

    #[tokio::test]
    async fn integration_test_kill_job() {
      let mut storage = setup_test_wrapper().await;

      storage
        .push(email_service::example_killed_email())
        .await
        .unwrap();

      let (job_id, res) = storage.execute_next().await.unwrap();
      assert_eq!(res, Err("AbortError: Invalid character.".to_owned()));
      apalis_core::sleep(Duration::from_secs(1)).await;
      let job = storage
        .fetch_by_id(&job_id)
        .await
        .unwrap()
        .expect("No job found");
      let ctx = job.parts.context;
      assert_eq!(*ctx.status(), State::Killed);
      // assert!(ctx.done_at().is_some());
      assert_eq!(
        ctx.last_error().clone().unwrap(),
        "{\"Err\":\"AbortError: Invalid character.\"}"
      );
    }

    #[tokio::test]
    async fn integration_test_acknowledge_good_job() {
      let mut storage = setup_test_wrapper().await;
      storage
        .push(email_service::example_good_email())
        .await
        .unwrap();

      let (job_id, res) = storage.execute_next().await.unwrap();
      assert_eq!(res, Ok("()".to_owned()));
      apalis_core::sleep(Duration::from_secs(1)).await;
      let job = storage.fetch_by_id(&job_id).await.unwrap().unwrap();
      let ctx = job.parts.context;
      assert_eq!(*ctx.status(), State::Done);
      assert!(ctx.done_at().is_some());
    }

    #[tokio::test]
    async fn integration_test_acknowledge_failed_job() {
      let mut storage = setup_test_wrapper().await;

      storage
        .push(email_service::example_retry_able_email())
        .await
        .unwrap();

      let (job_id, res) = storage.execute_next().await.unwrap();
      assert_eq!(
        res,
        Err("FailedError: Missing separator character '@'.".to_owned())
      );
      apalis_core::sleep(Duration::from_secs(1)).await;
      let job = storage.fetch_by_id(&job_id).await.unwrap().unwrap();
      let ctx = job.parts.context;
      assert_eq!(*ctx.status(), State::Failed);
      assert!(job.parts.attempt.current() >= 1);
      assert_eq!(
        ctx.last_error().clone().unwrap(),
        "{\"Err\":\"FailedError: Missing separator character '@'.\"}"
      );
    }

    #[tokio::test]
    async fn worker_consume() {
      use apalis_core::builder::WorkerBuilder;
      use apalis_core::builder::WorkerFactoryFn;
      let storage = $setup().await;
      let mut handle = storage.clone();

      let parts = handle
        .push(email_service::example_good_email())
        .await
        .unwrap();

      async fn task(_job: Email) -> &'static str {
        tokio::time::sleep(Duration::from_millis(100)).await;
        "Job well done"
      }
      let worker = WorkerBuilder::new("rango-tango").backend(storage);
      let worker = worker.build_fn(task);
      let wkr = worker.run();

      let w = wkr.get_handle();

      let runner = async move {
        apalis_core::sleep(Duration::from_secs(3)).await;
        let job_id = &parts.task_id;
        let job = get_job(&mut handle, job_id).await;
        let ctx = job.parts.context;

        assert_eq!(*ctx.status(), State::Done);
        assert!(ctx.done_at().is_some());
        assert!(ctx.lock_by().is_some());
        assert!(ctx.lock_at().is_some());
        assert!(ctx.last_error().is_some()); // TODO: rename last_error to last_result

        w.stop();
      };

      tokio::join!(runner, wkr);
    }

    #[tokio::test]
    async fn test_consume_jobs_with_priority() {
      let mut storage = setup_test_wrapper().await;

      // push several jobs with differing priorities, then ensure they get executed
      // in priority order.
      let job2 = push_email_priority(&mut storage, email_service::example_good_email(), 5).await;
      let job1 = push_email_priority(&mut storage, email_service::example_good_email(), 10).await;
      let job4 = push_email_priority(&mut storage, email_service::example_good_email(), -1).await;
      let job3 = push_email_priority(&mut storage, email_service::example_good_email(), 1).await;

      for (job_id, prio) in &[(job1, 10), (job2, 5), (job3, 1), (job4, -1)] {
        let (exec_job_id, res) = storage.execute_next().await.unwrap();
        assert_eq!(job_id, &exec_job_id);
        assert_eq!(res, Ok("()".to_owned()));
        apalis_core::sleep(Duration::from_millis(500)).await;
        let job = storage.fetch_by_id(&exec_job_id).await.unwrap().unwrap();
        let ctx = job.parts.context;

        assert_eq!(*ctx.status(), State::Done);
        assert_eq!(ctx.priority(), prio);
      }
    }
  };
}
