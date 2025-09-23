use axum::Router;
use axum::extract::{RawPathParams, Request};
use bytes::Bytes;
use http_body_util::{BodyExt, combinators::BoxBody};
use hyper::StatusCode;
use log::*;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use trailbase_wasm_common::{HttpContext, HttpContextKind, HttpContextUser};
use trailbase_wasm_runtime_host::{InitArgs, RuntimeOptions};

use crate::AppState;
use crate::User;
use crate::util::urlencode;

pub(crate) type AnyError = Box<dyn std::error::Error + Send + Sync>;

pub(crate) use trailbase_wasm_runtime_host::{KvStore, Runtime};

pub(crate) fn build_wasm_runtimes_for_components(
  n_threads: Option<usize>,
  conn: trailbase_sqlite::Connection,
  shared_kv_store: KvStore,
  components_path: PathBuf,
  fs_root_path: Option<PathBuf>,
) -> Result<Vec<Runtime>, AnyError> {
  let runtimes: Vec<Runtime> = std::fs::read_dir(&components_path).map_or_else(
    |_err| Ok(vec![]),
    |entries| {
      entries
        .into_iter()
        .flat_map(|entry| {
          let Ok(entry) = entry else {
            return None;
          };

          let Ok(metadata) = entry.metadata() else {
            return None;
          };

          if !metadata.is_file() {
            return None;
          }
          let path = entry.path();
          // let extension = path.extension().and_then(|e| e.to_str())?;

          if path.extension()? == "wasm" {
            return Some(Runtime::new(
              path,
              conn.clone(),
              shared_kv_store.clone(),
              RuntimeOptions {
                n_threads,
                fs_root_path: fs_root_path.clone(),
              },
            ));
          }
          return None;
        })
        .collect::<Result<Vec<Runtime>, _>>()
    },
  )?;

  if runtimes.is_empty() {
    debug!("No WASM component found in {components_path:?}");
  }

  return Ok(runtimes);
}

pub(crate) async fn install_routes_and_jobs(
  state: &AppState,
  runtime: Arc<RwLock<Runtime>>,
) -> Result<Option<Router<AppState>>, AnyError> {
  use trailbase_wasm_runtime_host::Error as WasmError;
  use trailbase_wasm_runtime_host::exports::trailbase::runtime::init_endpoint::MethodType;

  let version = state.version().git_version_tag.clone();

  let init_result = runtime
    .read()
    .await
    .call(async move |instance| {
      return instance.call_init(InitArgs { version }).await;
    })
    .await??;

  for (name, spec) in &init_result.job_handlers {
    let schedule = cron::Schedule::from_str(spec)?;
    let runtime = runtime.clone();
    let name_clone = name.to_string();

    let Some(job) = state.jobs().new_job(
      None,
      name,
      schedule,
      crate::scheduler::build_callback(move || {
        let name = name_clone.clone();
        let runtime = runtime.clone();

        return async move {
          runtime
            .read()
            .await
            .call(async move |instance| -> Result<(), WasmError> {
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
                    registered_path: name.clone(),
                    path_params: vec![],
                    user: None,
                  })?,
                )
                .body(empty())
                .map_err(|err| WasmError::Other(err.to_string()))?;

              instance.call_incoming_http_handler(request).await?;

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
  for (method, path) in &init_result.http_handlers {
    let runtime = runtime.clone();

    debug!("Installing WASM route: {method:?}: {path}");

    let handler = {
      let path = path.clone();

      move |params: RawPathParams, user: Option<User>, req: Request| async move {
        debug!(
          "Host received WASM HTTP request: {params:?}, {user:?}, {}",
          req.uri()
        );

        let result = runtime
          .read()
          .await
          .call(
            async move |instance| -> Result<axum::response::Response, WasmError> {
              let (mut parts, body) = req.into_parts();
              let bytes = body
                .collect()
                .await
                .map_err(|_err| WasmError::ChannelClosed)?
                .to_bytes();

              parts.headers.insert(
                "__context",
                to_header_value(&HttpContext {
                  kind: HttpContextKind::Http,
                  registered_path: path.clone(),
                  path_params: params
                    .iter()
                    .map(|(name, value)| (name.to_string(), value.to_string()))
                    .collect(),
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

              let response = instance.call_incoming_http_handler(request).await?;

              let (parts, body) = response.into_parts();
              let bytes = body
                .collect()
                .await
                .map_err(|_err| WasmError::ChannelClosed)?
                .to_bytes();

              return Ok(axum::response::Response::from_parts(parts, bytes.into()));
            },
          )
          .await;

        return match result {
          Ok(Ok(r)) => r,
          Ok(Err(err)) => internal_error_response(err),
          Err(err) => internal_error_response(err),
        };
      }
    };

    router = router.route(
      path,
      match method {
        MethodType::Delete => axum::routing::delete(handler),
        MethodType::Get => axum::routing::get(handler),
        MethodType::Head => axum::routing::head(handler),
        MethodType::Options => axum::routing::options(handler),
        MethodType::Patch => axum::routing::patch(handler),
        MethodType::Post => axum::routing::post(handler),
        MethodType::Put => axum::routing::put(handler),
        MethodType::Trace => axum::routing::trace(handler),
        MethodType::Connect => axum::routing::connect(handler),
      },
    );
  }

  return Ok(Some(router));
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

fn internal_error_response(err: impl std::string::ToString) -> axum::response::Response {
  return axum::response::Response::builder()
    .status(StatusCode::INTERNAL_SERVER_ERROR)
    .body(err.to_string().into())
    .unwrap_or_default();
}
