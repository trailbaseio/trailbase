#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]

use axum::Router;
use axum::extract::{RawPathParams, Request, State};
use axum::http::request::Parts;
use bytes::Bytes;
use futures_util::future::BoxFuture;
use http_body_util::{BodyExt, combinators::UnsyncBoxBody};
use hyper::StatusCode;
use log::*;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use trailbase_wasm_common::manifest::{
  HttpRoute as HttpRouteManifest, InitManifest, Job as JobManifest,
};
use trailbase_wasm_common::{HttpContext, HttpContextKind, HttpContextUser};
use trailbase_wasm_runtime_host::{
  Error as WasmError, InitArgs, RuntimeOptions, find_wasm_components,
};

pub use trailbase_wasm_runtime_host::functions::{SqliteFunctions, SqliteStore};
pub use trailbase_wasm_runtime_host::{HttpStore, KvStore, Runtime, SharedState};

pub type AnyError = Box<dyn std::error::Error + Send + Sync>;

pub async fn build_sync_wasm_runtimes_for_components(
  path_to_components: PathBuf,
  fs_root_path: Option<&Path>,
  use_winch: bool,
) -> Result<Vec<(SqliteStore, SqliteFunctions)>, AnyError> {
  let components = find_wasm_components(&path_to_components);
  let shared_state = Arc::new(SharedState {
    conn: None,
    kv_store: KvStore::new(),
    fs_root_path: None,
  });

  let mut sync_runtimes: Vec<(SqliteStore, SqliteFunctions)> = vec![];

  for path in components {
    let rt = Runtime::init(
      path,
      shared_state.clone(),
      RuntimeOptions {
        fs_root_path: fs_root_path.map(|p| p.to_owned()),
        // https://github.com/trailbaseio/trailbase/issues/206
        use_winch: if cfg!(target_os = "macos") {
          false
        } else {
          use_winch
        },
        tokio_runtime: None,
      },
    )?;

    // Create shared state. In the future we might want to instantiate multiple to avoid cross
    // SQLite-connection/thread synchronization.
    let store = SqliteStore::new(&rt).await?;
    let functions = store
      .initialize_sqlite_functions(trailbase_wasm_runtime_host::InitArgs { version: None })
      .await?;

    if !functions.is_empty() {
      sync_runtimes.push((store, functions));
    }
  }

  return Ok(sync_runtimes);
}

pub type WasmRuntimeBuilder = dyn Fn() -> Result<Runtime, WasmError> + Send + Sync;

pub fn wasm_runtime_builder(
  path_to_component: PathBuf,
  shared_state: Arc<SharedState>,
  tokio_runtime: Option<tokio::runtime::Handle>,
  runtime_root_fs: Option<PathBuf>,
  dev: bool,
) -> Box<WasmRuntimeBuilder> {
  return Box::new(move || {
    return Runtime::init(
      path_to_component.clone(),
      shared_state.clone(),
      RuntimeOptions {
        fs_root_path: runtime_root_fs.clone(),
        // https://github.com/trailbaseio/trailbase/issues/206
        use_winch: if cfg!(target_os = "macos") {
          false
        } else {
          dev
        },
        tokio_runtime: tokio_runtime.clone(),
      },
    );
  });
}

pub fn wasm_runtime_builders(
  path_to_components: PathBuf,
  conn: trailbase_sqlite::Connection,
  tokio_runtime: Option<tokio::runtime::Handle>,
  runtime_root_fs: Option<PathBuf>,
  shared_kv_store: Option<KvStore>,
  dev: bool,
) -> Vec<Box<WasmRuntimeBuilder>> {
  let shared_state = Arc::new(SharedState {
    conn: Some(conn),
    kv_store: shared_kv_store.unwrap_or_default(),
    fs_root_path: runtime_root_fs.clone(),
  });

  let components = find_wasm_components(&path_to_components);
  if components.is_empty() {
    debug!("No WASM component found in {path_to_components:?}");
  }

  return components
    .into_iter()
    .map(|path| {
      return wasm_runtime_builder(
        path,
        shared_state.clone(),
        tokio_runtime.clone(),
        runtime_root_fs.clone(),
        dev,
      );
    })
    .collect();
}

pub struct Job {
  pub name: String,
  pub schedule: cron::Schedule,
  pub callback: Box<dyn Fn() -> BoxFuture<'static, Result<(), AnyError>> + Send + Sync>,
}

pub struct InstallResult<S: Clone + Send + Sync> {
  pub router: Option<Router<S>>,
  pub jobs: Vec<Job>,
}

pub async fn install_routes_and_jobs<S: Clone + Send + Sync + 'static>(
  runtime: Arc<RwLock<Runtime>>,
  user_fn: for<'a> fn(&'a mut Parts, &'a S) -> BoxFuture<'a, Option<HttpContextUser>>,
  version: Option<String>,
) -> Result<InstallResult<S>, AnyError> {
  let InitManifest {
    http_handlers,
    job_handlers,
    sqlite_functions: _,
  } = {
    let store = HttpStore::new(&*runtime.read().await).await?;
    store.initialize(InitArgs { version }).await?
  };

  let http_handlers = http_handlers.unwrap_or_default();
  let job_handlers = job_handlers.unwrap_or_default();

  debug!(
    "Got {m} jobs and {n} http routes",
    m = job_handlers.len(),
    n = http_handlers.len()
  );

  let mut jobs: Vec<Job> = vec![];
  for JobManifest { name, spec } in job_handlers {
    let schedule = cron::Schedule::from_str(&spec)?;
    let store = HttpStore::new(&*runtime.read().await).await?;

    jobs.push(Job {
      name: name.clone(),
      schedule,
      callback: Box::new(move || {
        let name = name.clone();
        let store = store.clone();

        return Box::pin(async move {
          let uri = hyper::http::Uri::from_str(&format!("http://__job/?name={}", urlencode(&name)))
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

          store.call_incoming_http_handler(request).await?;

          Ok::<_, AnyError>(())
        });
      }),
    });
  }

  let mut router: Option<Router<S>> = None;
  for HttpRouteManifest { method, path } in http_handlers {
    debug!("Installing WASM route: {method:?}: {path}");

    // let runtime = runtime.clone();
    let store = HttpStore::new(&*runtime.read().await).await?;
    let registered_path = path.clone();

    use axum::response::Response;

    let handler =
      async move |params: RawPathParams, State(state): State<S>, req: Request| -> Response {
        // Construct WASI request form hyper/axum request.
        let (mut parts, body) = req.into_parts();

        let Ok(header_value) = to_header_value(&HttpContext {
          kind: HttpContextKind::Http,
          registered_path,
          path_params: params
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
          user: user_fn(&mut parts, &state).await,
        }) else {
          return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("header encoding failed".into())
            .unwrap_or_default();
        };

        parts.headers.insert("__context", header_value);

        let request = hyper::Request::from_parts(
          parts,
          UnsyncBoxBody::new(
            // NOTE: Ideally we'd stream the request body, however there's no way for us to re-map
            // axum::Error to hyper::Error. All hyper::Error's constructors are private. This is
            // likely an oversight in wasi_http.
            http_body_util::Full::new({
              let Ok(body) = body.collect().await else {
                return internal("request buffering failed");
              };
              body.to_bytes()
            })
            // Remapping Body impl's error type from infallible to hyper::Error.
            .map_err(|_| unreachable!()),
          ),
        );

        // Call WASM.
        return match store.call_incoming_http_handler(request).await {
          Ok(response) => {
            // Construct hyper/axum response from WASI response.
            let (parts, body) = response.into_parts();

            Response::from_parts(
              parts,
              axum::body::Body::from_stream(body.into_data_stream()),
            )
          }
          Err(err) => {
            warn!("`Error calling WASM component - call_incoming_http_handler` returned: {err}");
            return internal("component responded unexpectedly");
          }
        };
      };

    router = Some(
      router
        .take()
        .unwrap_or_else(|| Router::<S>::new())
        .route(&path, axum::routing::on(axum_method2(method), handler)),
    );
  }

  return Ok(InstallResult { router, jobs });
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

#[inline]
fn axum_method2(
  method: trailbase_wasm_common::manifest::HttpMethodType,
) -> axum::routing::MethodFilter {
  use trailbase_wasm_common::manifest::HttpMethodType;

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

fn empty() -> UnsyncBoxBody<Bytes, hyper::Error> {
  return UnsyncBoxBody::new(http_body_util::Empty::new().map_err(|_| unreachable!()));
}

fn internal(msg: &'static str) -> axum::response::Response {
  return axum::response::Response::builder()
    .status(StatusCode::INTERNAL_SERVER_ERROR)
    .body(msg.into())
    .unwrap_or_default();
}

fn to_header_value(
  context: &HttpContext,
) -> Result<hyper::http::HeaderValue, trailbase_wasm_runtime_host::Error> {
  return hyper::http::HeaderValue::from_bytes(&serde_json::to_vec(&context).unwrap_or_default())
    .map_err(|_err| trailbase_wasm_runtime_host::Error::Encoding);
}

fn urlencode(s: &str) -> String {
  return form_urlencoded::byte_serialize(s.as_bytes()).collect();
}
