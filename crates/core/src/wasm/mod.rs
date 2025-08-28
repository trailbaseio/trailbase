use axum::Router;
use axum::extract::{RawPathParams, Request};
use bytes::Bytes;
use http_body_util::{BodyExt, combinators::BoxBody};
use hyper::StatusCode;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use trailbase_wasm_common::{HttpContext, HttpContextKind, HttpContextUser};
use trailbase_wasm_runtime_host::exports::trailbase::runtime::init_endpoint::MethodType;
use trailbase_wasm_runtime_host::{Error as WasmError, KvStore, Runtime};

use crate::AppState;
use crate::User;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

pub(crate) fn build_wasm_runtimes_for_components(
  conn: trailbase_sqlite::Connection,
  components_path: PathBuf,
  fs_root_path: Option<PathBuf>,
) -> Result<Vec<Arc<Runtime>>, AnyError> {
  let shared_kv_store = KvStore::new();
  let mut runtimes: Vec<Arc<Runtime>> = vec![];

  if let Ok(entries) = std::fs::read_dir(components_path) {
    for entry in entries {
      let Ok(entry) = entry else {
        continue;
      };

      let Ok(metadata) = entry.metadata() else {
        continue;
      };

      if !metadata.is_file() {
        continue;
      }
      let path = entry.path();
      let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
        continue;
      };

      if extension == "wasm" {
        runtimes.push(Arc::new(Runtime::new(
          2,
          path,
          conn.clone(),
          shared_kv_store.clone(),
          fs_root_path.clone(),
        )?));
      }
    }
  }

  if runtimes.is_empty() {
    log::debug!("No WASM component found");
  }

  return Ok(runtimes);
}

pub(crate) async fn install_routes_and_jobs(
  state: &AppState,
  runtime: Arc<Runtime>,
) -> Result<Option<Router<AppState>>, AnyError> {
  let init_result = runtime
    .call(async |instance| {
      return instance.call_init().await;
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
            .call(async move |instance| -> Result<(), WasmError> {
              let request = hyper::Request::builder()
                // NOTE: We cannot use a custom-scheme, since the wasi http
                // implementation rejects everything but http and https.
                .uri(format!("http://__job/?name={name}"))
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
                .unwrap_or_default();

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

  log::debug!("Got {} WASM routes", init_result.http_handlers.len());

  let mut router = Router::<AppState>::new();
  for (method, path) in &init_result.http_handlers {
    let runtime = runtime.clone();

    log::debug!("Installing WASM route: {method:?}: {path}");

    let handler = {
      let path = path.clone();

      move |params: RawPathParams, user: Option<User>, req: Request| async move {
        log::debug!(
          "Host received WASM HTTP request: {params:?}, {user:?}, {}",
          req.uri()
        );

        return runtime
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
          .await
          .flatten()
          .unwrap_or_else(|err| {
            return axum::response::Response::builder()
              .status(StatusCode::INTERNAL_SERVER_ERROR)
              .body(err.to_string().into())
              .unwrap_or_default();
          });
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

fn to_header_value(context: &HttpContext) -> Result<hyper::http::HeaderValue, WasmError> {
  return hyper::http::HeaderValue::from_bytes(&serde_json::to_vec(&context).unwrap_or_default())
    .map_err(|_err| WasmError::Encoding);
}
