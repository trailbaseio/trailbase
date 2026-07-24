use axum::Router;
use std::sync::Arc;
use tokio::sync::RwLock;
use trailbase_wasm_common::HttpContextUser;
use trailbase_wasm_common::manifest::Metadata;
use trailbase_wasm_runtime_axum::Job;

use crate::{AppState, User};

pub(crate) use trailbase_wasm_runtime_axum::{
  AnyError, KvStore, Runtime, SqliteFunctions, SqliteStore, WasmRuntimeBuilder,
  build_sync_wasm_runtimes_for_components, wasm_runtime_builders,
};

pub(crate) async fn install_routes_and_jobs(
  state: &AppState,
  runtime: Arc<RwLock<(Option<Metadata>, Runtime)>>,
) -> Result<Option<Router<AppState>>, AnyError> {
  use axum::extract::OptionalFromRequestParts;
  use axum::http::request::Parts;
  use trailbase_wasm_runtime_axum::{InstallResult, install_routes_and_jobs};

  fn extract_user<'a>(
    parts: &'a mut Parts,
    s: &'a AppState,
  ) -> futures_util::future::BoxFuture<'a, Option<HttpContextUser>> {
    return Box::pin(async {
      User::from_request_parts(parts, s)
        .await
        .ok()
        .flatten()
        .map(|u| HttpContextUser {
          id: u.id,
          email: u.email,
          username: u.username,
          csrf_token: u.csrf_token,
        })
    });
  }

  let version = state.version().git_version_tag.clone();

  let mut metadata_and_rt = runtime.write().await;
  let component_name = metadata_and_rt
    .1
    .component_path()
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or("unknown")
    .to_string();

  let InstallResult {
    router,
    jobs,
    metadata,
  } = install_routes_and_jobs::<AppState>(&metadata_and_rt.1, extract_user, version).await?;

  if let Some(metadata) = metadata {
    log::debug!("Registering metadata manifest for WASM component '{component_name}'");
    let _ = metadata_and_rt.0.insert(metadata);
  }

  for Job {
    name,
    schedule,
    callback,
  } in jobs
  {
    let Some(job) = state.jobs().new_job(None, name, schedule, callback) else {
      return Err("Failed to add job".into());
    };

    job.start();
  }

  return Ok(router);
}
