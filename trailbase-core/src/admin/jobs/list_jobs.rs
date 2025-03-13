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
  pub latest: Option<(i64, Option<String>)>,
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
    .map(|t| {
      let latest = t.latest().map(|l| (l.0.timestamp(), l.1));
      let enabled = t.running();

      return Job {
        id: t.id,
        name: t.name(),
        schedule: t.schedule().to_string(),

        enabled,
        next: t.next_run().map(|t| t.timestamp()),
        latest,
      };
    })
    .collect();

  return Ok(Json(ListJobsResponse { jobs }));
}
