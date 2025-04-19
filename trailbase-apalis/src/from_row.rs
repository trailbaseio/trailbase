use apalis_core::request::Parts;
use apalis_core::task::attempt::Attempt;
use apalis_core::task::task_id::TaskId;
use apalis_core::{request::Request, worker::WorkerId};

use crate::context::SqlContext;

pub(crate) type SqlRequest<T> = Request<T, SqlContext>;

pub(crate) fn from_row(row: &rusqlite::Row) -> Result<SqlRequest<String>, trailbase_sqlite::Error> {
  use chrono::DateTime;
  use std::str::FromStr;

  let job: String = row.get("job")?;
  let task_id: TaskId = TaskId::from_str(&row.get::<_, String>("id")?)
    .map_err(|e| trailbase_sqlite::Error::Other(e.into()))?;
  let mut parts = Parts::<SqlContext>::default();
  parts.task_id = task_id;

  let attempt: i32 = row.get("attempts").unwrap_or(0);
  parts.attempt = Attempt::new_with_value(attempt as usize);

  let mut context = crate::context::SqlContext::new();

  let run_at: i64 = row.get("run_at")?;
  context.set_run_at(DateTime::from_timestamp(run_at, 0).unwrap_or_default());

  if let Ok(max_attempts) = row.get("max_attempts") {
    context.set_max_attempts(max_attempts)
  }

  let done_at: Option<i64> = row.get("done_at").unwrap_or_default();
  context.set_done_at(done_at);

  let lock_at: Option<i64> = row.get("lock_at").unwrap_or_default();
  context.set_lock_at(lock_at);

  let last_error = row.get("last_error").unwrap_or_default();
  context.set_last_error(last_error);

  let status: String = row.get("status")?;
  context.set_status(
    status
      .parse()
      .map_err(|_| trailbase_sqlite::Error::Other("parse failed".into()))?,
  );

  let lock_by: Option<String> = row.get("lock_by").unwrap_or_default();
  context.set_lock_by(
    lock_by
      .as_deref()
      .map(WorkerId::from_str)
      .transpose()
      .map_err(|_| trailbase_sqlite::Error::Other("transpose failed".into()))?,
  );

  let priority: i32 = row.get("priority").unwrap_or_default();
  context.set_priority(priority);

  parts.context = context;
  Ok(SqlRequest { args: job, parts })
}
