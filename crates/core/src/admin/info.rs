use axum::{Json, extract::State};
use serde::Serialize;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;

#[derive(Clone, Debug, Default, Serialize, TS)]
#[ts(export)]
pub struct InfoResponse {
  /// Build metadata.
  compiler: Option<String>,
  /// Git metadata
  commit_hash: Option<String>,
  commit_date: Option<String>,
  git_version: Option<(String, usize)>,
  /// Runtime metadata.
  threads: usize,
  command_line_arguments: Option<Vec<String>>,
  /// Start time in seconds since epoch,
  start_time: u64,
}

pub async fn info_handler(State(state): State<AppState>) -> Result<Json<InfoResponse>, Error> {
  return Ok(Json(build_info_response(&state)));
}

fn build_info_response(state: &AppState) -> InfoResponse {
  let version_info = state.version();
  let git_version = version_info
    .git_version()
    .map(|v| (v.tag(), v.commits_since.unwrap_or(0) as usize));

  return InfoResponse {
    compiler: version_info.host_compiler,
    commit_hash: version_info.git_commit_hash,
    commit_date: version_info.git_commit_date,
    git_version,
    threads: std::thread::available_parallelism().map_or(0, |v| v.into()),
    command_line_arguments: Some(std::env::args().collect()),
    start_time: state
      .start_time()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_secs(),
  };
}
