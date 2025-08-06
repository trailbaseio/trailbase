use apalis_core::request::Request;
use apalis_core::service_fn::FromRequest;
use apalis_core::worker::WorkerId;
use apalis_core::{error::Error, request::State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The context for a job is represented here
/// Used to provide a context for a job with an sql backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlContext {
  status: State,
  run_at: DateTime<Utc>,
  max_attempts: i32,
  last_error: Option<String>,
  lock_at: Option<i64>,
  lock_by: Option<WorkerId>,
  done_at: Option<i64>,
  priority: i32,
}

impl Default for SqlContext {
  fn default() -> Self {
    Self::new()
  }
}

impl SqlContext {
  /// Build a new context with defaults
  pub fn new() -> Self {
    SqlContext {
      status: State::Pending,
      run_at: Utc::now(),
      lock_at: None,
      done_at: None,
      max_attempts: 5,
      last_error: None,
      lock_by: None,
      priority: 0,
    }
  }

  /// Set the number of attempts
  pub fn set_max_attempts(&mut self, max_attempts: i32) {
    self.max_attempts = max_attempts;
  }

  /// Gets the maximum attempts for a job. Default 25
  pub fn max_attempts(&self) -> i32 {
    self.max_attempts
  }

  /// Get the time a job was done
  pub fn done_at(&self) -> &Option<i64> {
    &self.done_at
  }

  /// Set the time a job was done
  pub fn set_done_at(&mut self, done_at: Option<i64>) {
    self.done_at = done_at;
  }

  /// Get the time a job is supposed to start
  pub fn run_at(&self) -> &DateTime<Utc> {
    &self.run_at
  }

  /// Set the time a job should run
  pub fn set_run_at(&mut self, run_at: DateTime<Utc>) {
    self.run_at = run_at;
  }

  /// Get the time a job was locked
  pub fn lock_at(&self) -> &Option<i64> {
    &self.lock_at
  }

  /// Set the lock_at value
  pub fn set_lock_at(&mut self, lock_at: Option<i64>) {
    self.lock_at = lock_at;
  }

  /// Get the job status
  pub fn status(&self) -> &State {
    &self.status
  }

  /// Set the job status
  pub fn set_status(&mut self, status: State) {
    self.status = status;
  }

  /// Get the time a job was locked
  pub fn lock_by(&self) -> &Option<WorkerId> {
    &self.lock_by
  }

  /// Set `lock_by`
  pub fn set_lock_by(&mut self, lock_by: Option<WorkerId>) {
    self.lock_by = lock_by;
  }

  /// Get the time a job was locked
  pub fn last_error(&self) -> &Option<String> {
    &self.last_error
  }

  /// Set the last error
  pub fn set_last_error(&mut self, error: Option<String>) {
    self.last_error = error;
  }

  /// Set the job priority. Larger values will run sooner. Default is 0.
  pub fn set_priority(&mut self, priority: i32) {
    self.priority = priority
  }

  /// Get the job priority
  pub fn priority(&self) -> &i32 {
    &self.priority
  }
}

impl<Req> FromRequest<Request<Req, SqlContext>> for SqlContext {
  fn from_request(req: &Request<Req, SqlContext>) -> Result<Self, Error> {
    Ok(req.parts.context.clone())
  }
}
