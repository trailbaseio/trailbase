use axum::{extract::State, Json};
use log::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::AppState;

#[derive(Debug, Serialize, TS)]
pub struct Task {
  pub id: i32,
  pub name: String,
  pub schedule: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ListTasksResponse {
  pub tasks: Vec<Task>,
}

pub async fn list_tasks_handler(
  State(state): State<AppState>,
) -> Result<Json<ListTasksResponse>, Error> {
  let tasks: Vec<_> = state
    .tasks()
    .tasks
    .lock()
    .values()
    .map(|t| Task {
      id: t.id,
      name: t.name.clone(),
      schedule: t.schedule.to_string(),
    })
    .collect();

  return Ok(Json(ListTasksResponse { tasks }));
}
