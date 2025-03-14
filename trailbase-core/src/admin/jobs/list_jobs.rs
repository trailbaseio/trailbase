use axum::{extract::State, Json};
use serde::Serialize;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::AppState;

#[derive(Debug, Serialize, TS)]
pub struct Job {
  pub id: i32,
  pub name: String,
  pub schedule: String,

  pub enabled: bool,
  pub next: Option<i64>,
  /// Optional metadata from latest run: start timestamp in seconds since epoch, duration in
  /// milliseconds and error output.
  pub latest: Option<(i64, i64, Option<String>)>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ListJobsResponse {
  pub jobs: Vec<Job>,
}

pub async fn list_jobs_handler(
  State(state): State<AppState>,
) -> Result<Json<ListJobsResponse>, Error> {
  let jobs: Vec<_> = state
    .jobs()
    .jobs
    .lock()
    .values()
    .map(|job| {
      let latest = job
        .latest()
        .map(|l| (l.0.timestamp(), l.1.num_milliseconds(), l.2));
      let enabled = job.running();

      return Job {
        id: job.id,
        name: job.name(),
        schedule: job.schedule().to_string(),

        enabled,
        next: job.next_run().map(|t| t.timestamp()),
        latest,
      };
    })
    .collect();

  return Ok(Json(ListJobsResponse { jobs }));
}
