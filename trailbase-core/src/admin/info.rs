use axum::{Json, extract::State};
use serde::Serialize;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;

#[derive(Debug, Default, Serialize, TS)]
#[ts(export)]
pub struct InfoResponse {
  version: String,
  compiler: Option<String>,
  commit_hash: Option<String>,
  commit_date: Option<String>,
  threads: usize,
}

pub async fn info_handler(State(state): State<AppState>) -> Result<Json<InfoResponse>, Error> {
  let version_info = state.version();
  let version = format!(
    "{major}.{minor}.{patch}",
    major = version_info.major,
    minor = version_info.minor,
    patch = version_info.patch
  );

  return Ok(Json(InfoResponse {
    version,
    compiler: version_info.host_compiler,
    commit_hash: version_info.commit_hash,
    commit_date: version_info.commit_date,
    threads: std::thread::available_parallelism().map_or(0, |v| v.into()),
  }));
}
