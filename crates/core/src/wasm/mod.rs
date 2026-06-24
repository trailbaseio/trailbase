use axum::Router;
use std::sync::Arc;
use tokio::sync::RwLock;
use trailbase_wasm_common::HttpContextUser;
use trailbase_wasm_runtime_axum::Job;

use crate::{AppState, User};

pub(crate) use trailbase_wasm_runtime_axum::{
  AnyError, KvStore, Runtime, SqliteFunctions, SqliteStore, WasmRuntimeBuilder,
  build_sync_wasm_runtimes_for_components, wasm_runtime_builders,
};

/// Probe a WASM component's manifest endpoint by calling it directly
/// through the runtime's HTTP store. Returns the parsed manifest or None
/// if the component doesn't expose one or the response is invalid.
pub(crate) async fn probe_manifest(
  runtime: &Runtime,
  manifest_path: &str,
) -> Option<crate::app_state::WasmManifest> {
  let manifest_uri = format!("http://localhost{manifest_path}");

  let manifest_store = HttpStore::new(runtime).await.ok()?;
  let context_header = to_header_value(&HttpContext {
    kind: HttpContextKind::Http,
    registered_path: manifest_path.to_string(),
    path_params: vec![],
    user: None,
  })
  .ok()?;

  let probe_req = hyper::Request::builder()
    .method(hyper::Method::GET)
    .uri(manifest_uri)
    .header("__context", context_header)
    .body(empty())
    .ok()?;

  let resp = manifest_store.call_incoming_http_handler(probe_req).await.ok()?;

  if resp.status() != hyper::StatusCode::OK {
    return None;
  }

  let (_, body) = resp.into_parts();
  let collected = body.collect().await.ok()?;

  match serde_json::from_slice::<crate::app_state::WasmManifest>(&collected.to_bytes()) {
    Ok(manifest) => Some(manifest),
    Err(err) => {
      warn!("Manifest at '{manifest_path}' returned invalid JSON: {err}");
      None
    }
  }
}

pub(crate) async fn install_routes_and_jobs(
  state: &AppState,
  runtime: Arc<RwLock<Runtime>>,
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

  let component_name = runtime
    .read()
    .await
    .component_path()
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or("unknown")
    .to_string();

  // Convention: manifest is always at /_/wasm/<file-stem>/manifest.
  // Components must register their routes under the same prefix as their file name.
  let manifest_path = format!("/_/wasm/{component_name}/manifest");
  if let Some(manifest) = probe_manifest(&*runtime.read().await, &manifest_path).await {
    info!("Registering manifest for WASM component '{component_name}'");
    state
      .wasm_manifests()
      .write()
      .await
      .insert(component_name, manifest);
  }

  for (name, spec) in init_result.job_handlers {
    let schedule = cron::Schedule::from_str(&spec)?;
    let store = HttpStore::new(&*runtime.read().await).await?;

  let InstallResult { router, jobs } =
    install_routes_and_jobs::<AppState>(runtime, extract_user, version).await?;

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
            // The host encodes user IDs with BASE64_URL_SAFE (with padding), but the
            // wasm-runtime-guest's is_admin() decodes with URL_SAFE_NO_PAD. Strip the
            // padding here so the guest receives the format it expects.
            id: u.id.trim_end_matches('=').to_string(),
            email: u.email,
            username: u.username,
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
            warn!("`Error calling WASM component - call_incoming_http_handler` returned: {err}");
            return internal("component responded unexpectedly");
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
