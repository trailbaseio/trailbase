use axum::Router;
use axum::extract::{RawPathParams, Request};
use bytes::Bytes;
use http_body_util::{BodyExt, combinators::UnsyncBoxBody};
use hyper::StatusCode;
use log::*;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use trailbase_wasm_common::{HttpContext, HttpContextKind, HttpContextUser};
use trailbase_wasm_runtime_host::{InitArgs, RuntimeOptions, find_wasm_components};

use crate::User;
use crate::util::urlencode;
use crate::{AppState, DataDir};

pub(crate) use trailbase_wasm_runtime_host::functions::{SqliteFunctions, SqliteStore};
pub(crate) use trailbase_wasm_runtime_host::{HttpStore, KvStore, Runtime, SharedState};

pub(crate) type AnyError = Box<dyn std::error::Error + Send + Sync>;

pub(crate) async fn build_sync_wasm_runtimes_for_components(
  components_path: PathBuf,
  fs_root_path: Option<&Path>,
  use_winch: bool,
) -> Result<Vec<(SqliteStore, SqliteFunctions)>, AnyError> {
  let components = find_wasm_components(&components_path);
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

    if !functions.scalar_functions.is_empty() {
      sync_runtimes.push((store, functions));
    }
  }

  return Ok(sync_runtimes);
}

pub(crate) type WasmRuntimeBuilder =
  Box<dyn Fn() -> Result<Vec<Runtime>, crate::wasm::AnyError> + Send + Sync>;

pub(crate) fn wasm_runtimes_builder(
  data_dir: DataDir,
  conn: trailbase_sqlite::Connection,
  rt: Option<tokio::runtime::Handle>,
  runtime_root_fs: Option<std::path::PathBuf>,
  shared_kv_store: Option<KvStore>,
  dev: bool,
) -> Result<WasmRuntimeBuilder, AnyError> {
  let components_path = data_dir.root().join("wasm");

  let shared_state = Arc::new(SharedState {
    conn: Some(conn),
    kv_store: shared_kv_store.unwrap_or_default(),
    fs_root_path: runtime_root_fs.clone(),
  });

  return Ok(Box::new(move || {
    let components = find_wasm_components(&components_path);
    if components.is_empty() {
      debug!("No WASM component found in {components_path:?}");
      return Ok(vec![]);
    }

    let runtimes: Vec<Runtime> = components
      .into_iter()
      .map(|path| {
        return Runtime::init(
          path,
          shared_state.clone(),
          RuntimeOptions {
            fs_root_path: runtime_root_fs.clone(),
            // https://github.com/trailbaseio/trailbase/issues/206
            use_winch: if cfg!(target_os = "macos") {
              false
            } else {
              dev
            },
            tokio_runtime: rt.clone(),
          },
        );
      })
      .collect::<Result<Vec<_>, _>>()?;

    return Ok(runtimes);
  }));
}

pub(crate) async fn install_routes_and_jobs(
  state: &AppState,
  runtime: Arc<RwLock<Runtime>>,
) -> Result<Option<Router<AppState>>, AnyError> {
  use trailbase_wasm_runtime_host::Error as WasmError;

  let init_result = {
    let store = HttpStore::new(&*runtime.read().await).await?;
    store
      .initialize(InitArgs {
        version: state.version().git_version_tag.clone(),
      })
      .await?
  };

  for (name, spec) in init_result.job_handlers {
    let schedule = cron::Schedule::from_str(&spec)?;
    let store = HttpStore::new(&*runtime.read().await).await?;

    let Some(job) = state.jobs().new_job(
      None,
      name.clone(),
      schedule,
      crate::scheduler::build_callback(move || {
        let name = name.clone();
        let store = store.clone();

        return async move {
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

    // let runtime = runtime.clone();
    let store = HttpStore::new(&*runtime.read().await).await?;
    let registered_path = path.clone();

    use axum::response::Response;

    let handler =
      async move |params: RawPathParams, user: Option<User>, req: Request| -> Response {
        // Construct WASI request form hyper/axum request.
        let (mut parts, body) = req.into_parts();

        let Ok(header_value) = to_header_value(&HttpContext {
          kind: HttpContextKind::Http,
          registered_path,
          path_params: params
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
          user: user.map(|u| HttpContextUser {
            id: u.id,
            email: u.email,
            csrf_token: u.csrf_token,
          }),
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
            debug!("`call_incoming_http_handler` returned: {err}");
            return internal("call");
          }
        };
      };

    router = router.route(&path, axum::routing::on(axum_method(method), handler));
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
