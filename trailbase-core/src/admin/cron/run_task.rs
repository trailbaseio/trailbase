use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct RunTaskRequest {
  id: i32,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct RunTaskResponse {
  error: Option<String>,
}

pub async fn run_tasks_handler(
  State(state): State<AppState>,
  Json(request): Json<RunTaskRequest>,
) -> Result<Json<RunTaskResponse>, Error> {
  let callback = {
    let tasks = state.tasks();
    let lock = tasks.tasks.lock();
    let Some(task) = lock.get(&request.id) else {
      return Err(Error::Precondition("Not found".into()));
    };

    task.callback.clone()
  };

  let result = callback().await;

  return Ok(Json(RunTaskResponse {
    error: result.err().map(|e| e.to_string()),
  }));
}
