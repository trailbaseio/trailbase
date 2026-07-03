use axum::Router;
use axum::http::request::Parts;
use clap::Parser;
use futures_util::future::BoxFuture;
use std::sync::Arc;
use trailbase_sqlite::Connection;
use trailbase_wasm_common::HttpContextUser;
use trailbase_wasm_runtime_axum::{
  InstallResult, SharedState, install_routes_and_jobs, wasm_runtime_builder,
};

#[derive(Debug, Clone)]
pub struct State;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None, disable_version_flag = true)]
pub struct CommandLineArgs {
  #[arg(long, env)]
  pub path: std::path::PathBuf,

  #[arg(long, env, default_value = "3000")]
  pub port: u16,
}

fn extract_user(_parts: &mut Parts, _state: &State) -> BoxFuture<'static, Option<HttpContextUser>> {
  return Box::pin(async { None });
}

#[tokio::main]
async fn main() {
  let args = CommandLineArgs::parse();

  env_logger::Builder::from_env(env_logger::Env::new().default_filter_or(
    "debug,tracing::span=warn,cranelift_codegen=warn,wasmtime_internal_cranelift=warn",
  ))
  .format_timestamp_micros()
  .init();

  let conn = Connection::open_in_memory().unwrap();

  let shared_state = Arc::new(SharedState {
    conn: Some(conn),
    kv_store: Default::default(),
    fs_root_path: None,
  });

  let runtimes_builder =
    wasm_runtime_builder(args.path, shared_state, None, None, /*dev=*/ false);
  let runtime = Arc::new(tokio::sync::RwLock::new(runtimes_builder().unwrap()));

  let mut router = Router::new();
  let InstallResult { router: r, jobs }: InstallResult<State> =
    install_routes_and_jobs::<State>(runtime, extract_user, None)
      .await
      .unwrap();

  if let Some(routes) = r {
    router = router.merge(routes);
  }

  if !jobs.is_empty() {
    log::info!("ignoring {} jobs", jobs.len());
  }

  let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", args.port))
    .await
    .unwrap();

  axum::serve(listener, router.with_state(State))
    .await
    .unwrap();
}
