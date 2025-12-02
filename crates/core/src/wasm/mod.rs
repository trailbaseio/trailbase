use axum::Router;
use axum::extract::{RawPathParams, Request};
use bytes::Bytes;
use http_body_util::{BodyExt, combinators::BoxBody};
use hyper::StatusCode;
use log::*;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use trailbase_wasm_common::{HttpContext, HttpContextKind, HttpContextUser};
use trailbase_wasm_runtime_host::{InitArgs, RuntimeOptions, SharedExecutor, load_wasm_components};

use crate::User;
use crate::util::urlencode;
use crate::{AppState, DataDir};

pub(crate) use trailbase_wasm_runtime_host::functions::SqliteFunctionRuntime;
pub(crate) use trailbase_wasm_runtime_host::{KvStore, Runtime};

pub(crate) type AnyError = Box<dyn std::error::Error + Send + Sync>;

pub(crate) fn build_sync_wasm_runtimes_for_components(
  components_path: PathBuf,
  fs_root_path: Option<&Path>,
  dev: bool,
) -> Result<Vec<SqliteFunctionRuntime>, AnyError> {
  let sync_runtimes: Vec<SqliteFunctionRuntime> =
    load_wasm_components(components_path, |path: std::path::PathBuf| {
      return SqliteFunctionRuntime::new(
        path,
        RuntimeOptions {
          fs_root_path: fs_root_path.map(|p| p.to_owned()),
          use_winch: dev,
        },
      );
    })?;

  return Ok(sync_runtimes);
}

pub(crate) fn build_wasm_runtimes_for_components(
  n_threads: Option<usize>,
  conn: trailbase_sqlite::Connection,
  shared_kv_store: KvStore,
  components_path: PathBuf,
  fs_root_path: Option<PathBuf>,
  dev: bool,
) -> Result<Vec<Runtime>, AnyError> {
  let executor = SharedExecutor::new(n_threads);

  let runtimes: Vec<Runtime> =
    load_wasm_components(components_path.clone(), |path: std::path::PathBuf| {
      return Runtime::new(
        executor.clone(),
        path,
        conn.clone(),
        shared_kv_store.clone(),
        RuntimeOptions {
          fs_root_path: fs_root_path.clone(),
          use_winch: dev,
        },
      );
    })?;

  if runtimes.is_empty() {
    debug!("No WASM component found in {components_path:?}");
  }

  return Ok(runtimes);
}

pub struct WasmRuntimeResult {
  pub shared_kv_store: KvStore,
  pub build_wasm_runtime:
    Box<dyn Fn() -> Result<Vec<Runtime>, crate::wasm::AnyError> + Send + Sync>,
}

pub fn build_wasm_runtime(
  data_dir: DataDir,
  conn: trailbase_sqlite::Connection,
  runtime_root_fs: Option<std::path::PathBuf>,
  runtime_threads: Option<usize>,
  dev: bool,
) -> Result<WasmRuntimeResult, AnyError> {
  let wasm_dir = data_dir.root().join("wasm");
  let shared_kv_store = KvStore::new();

  return Ok(WasmRuntimeResult {
    shared_kv_store: shared_kv_store.clone(),
    build_wasm_runtime: Box::new(move || {
      return crate::wasm::build_wasm_runtimes_for_components(
        runtime_threads,
        conn.clone(),
        shared_kv_store.clone(),
        wasm_dir.clone(),
        runtime_root_fs.clone(),
        dev,
      );
    }),
  });
}

pub(crate) async fn install_routes_and_jobs(
  state: &AppState,
  runtime: Arc<RwLock<Runtime>>,
) -> Result<Option<Router<AppState>>, AnyError> {
  use trailbase_wasm_runtime_host::Error as WasmError;

  let version = state.version().git_version_tag.clone();

  let init_result = runtime
    .read()
    .await
    .call(async move |runner| {
      return runner.initialize(InitArgs { version }).await;
    })
    .await??;

  for (name, spec) in init_result.job_handlers {
    let schedule = cron::Schedule::from_str(&spec)?;
    let runtime = runtime.clone();

    let Some(job) = state.jobs().new_job(
      None,
      name.clone(),
      schedule,
      crate::scheduler::build_callback(move || {
        let name = name.clone();
        let runtime = runtime.clone();

        return async move {
          runtime
            .read()
            .await
            .call(async move |runner| -> Result<(), WasmError> {
              let uri =
                hyper::http::Uri::from_str(&format!("http://__job/?name={}", urlencode(&name)))
                  .map_err(|err| WasmError::Other(format!("Job URI: {err}")))?;

              let request = hyper::Request::builder()
                // NOTE: We cannot use a custom-scheme, since the wasi http
                // implementation rejects everything but http and https.
                .uri(uri)
                .header(
                  "__context",
                  to_header_value(&HttpContext {
                    kind: HttpContextKind::Job,
                    registered_path: name,
                    path_params: vec![],
                    user: None,
                  })?,
                )
                .body(empty())
                .map_err(|err| WasmError::Other(err.to_string()))?;

              runner.call_incoming_http_handler(request).await?;

              return Ok(());
            })
            .await??;

          Ok::<_, AnyError>(())
        };
      }),
    ) else {
      return Err("Failed to add job".into());
    };

    job.start();
  }

  debug!("Got {} WASM routes", init_result.http_handlers.len());

  let mut router = Router::<AppState>::new();
  for (method, path) in init_result.http_handlers {
    debug!("Installing WASM route: {method:?}: {path}");

    let runtime = runtime.clone();
    let registered_path = path.clone();
    let log_message = format!("Uncaught wasm error {method:?}:{path}");

    router = router.route(
      &path,
      axum::routing::on(
        axum_method(method),
        |params: RawPathParams, user: Option<User>, req: Request| async move {
          use axum::response::Response;

          let result = runtime
            .read()
            .await
            .call(async move |runner| -> Result<Response, WasmError> {
              let (mut parts, body) = req.into_parts();
              let bytes = body
                .collect()
                .await
                .map_err(|_err| WasmError::ChannelClosed)?
                .to_bytes();

              let path_params = params
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect();

              parts.headers.insert(
                "__context",
                to_header_value(&HttpContext {
                  kind: HttpContextKind::Http,
                  registered_path,
                  path_params,
                  user: user.map(|u| HttpContextUser {
                    id: u.id,
                    email: u.email,
                    csrf_token: u.csrf_token,
                  }),
                })?,
              );

              let request = hyper::Request::from_parts(
                parts,
                BoxBody::new(http_body_util::Full::new(bytes).map_err(|_| unreachable!())),
              );

              // Call WASM.
              let (parts, body) = runner
                .call_incoming_http_handler(request)
                .await?
                .into_parts();

              return Ok(Response::from_parts(
                parts,
                body
                  .collect()
                  .await
                  .map_err(|_err| WasmError::ChannelClosed)?
                  .to_bytes()
                  .into(),
              ));
            })
            .await;

          return match result {
            Ok(Ok(r)) => r,
            Ok(Err(err)) => {
              debug!("{log_message}: {err}");

              return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body("failure".into())
                .unwrap_or_default();
            }
            Err(err) => {
              error!("Broken setup: {err}");

              return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body("failure".into())
                .unwrap_or_default();
            }
          };
        },
      ),
    );
  }

  return Ok(Some(router));
}

#[inline]
fn axum_method(method: trailbase_wasm_runtime_host::HttpMethodType) -> axum::routing::MethodFilter {
  use trailbase_wasm_runtime_host::HttpMethodType;

  return match method {
    HttpMethodType::Delete => axum::routing::MethodFilter::DELETE,
    HttpMethodType::Get => axum::routing::MethodFilter::GET,
    HttpMethodType::Head => axum::routing::MethodFilter::HEAD,
    HttpMethodType::Options => axum::routing::MethodFilter::OPTIONS,
    HttpMethodType::Patch => axum::routing::MethodFilter::PATCH,
    HttpMethodType::Post => axum::routing::MethodFilter::POST,
    HttpMethodType::Put => axum::routing::MethodFilter::PUT,
    HttpMethodType::Trace => axum::routing::MethodFilter::TRACE,
    HttpMethodType::Connect => axum::routing::MethodFilter::CONNECT,
  };
}

fn empty() -> BoxBody<Bytes, hyper::Error> {
  return BoxBody::new(http_body_util::Empty::new().map_err(|_| unreachable!()));
}

fn to_header_value(
  context: &HttpContext,
) -> Result<hyper::http::HeaderValue, trailbase_wasm_runtime_host::Error> {
  return hyper::http::HeaderValue::from_bytes(&serde_json::to_vec(&context).unwrap_or_default())
    .map_err(|_err| trailbase_wasm_runtime_host::Error::Encoding);
}
