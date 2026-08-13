use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::AppState;
use crate::admin::AdminError as Error;

#[derive(Debug, Deserialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct RunJobRequest {
  id: i32,
}

#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct RunJobResponse {
  error: Option<String>,
}

#[utoipa::path(
  post,
  path = "/job/run",
  tag = "admin",
  request_body = RunJobRequest,
  responses(
    (status = 200, description = "Success", body = RunJobResponse),
  )
)]
pub async fn run_job_handler(
  State(state): State<AppState>,
  Json(request): Json<RunJobRequest>,
) -> Result<Json<RunJobResponse>, Error> {
  let Some(result) = state.jobs().run_job(request.id).await else {
    return Err(Error::Precondition("Job not found".into()));
  };

  return Ok(Json(RunJobResponse {
    error: result.err().map(|e| e.to_string()),
  }));
}
