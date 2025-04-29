use axum::Router;
use axum::body::Body;
use axum::extract::{RawPathParams, Request};
use axum::http::{HeaderName, HeaderValue, request::Parts};
use axum::response::Response;
use futures_util::FutureExt;
use log::*;
use std::str::FromStr;
use tokio::sync::oneshot;

use trailbase_js::runtime::{
  DispatchArgs, Error as RSError, JsHttpResponse, JsHttpResponseError, JsUser, Message, Module,
  Runtime, RuntimeHandle, build_call_async_js_function_message, build_http_dispatch_message,
  get_arg,
};

use crate::AppState;
use crate::auth::user::User;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

/// Get's called from JS during `addRoute` and installs an axum HTTP handler.
///
/// The axum HTTP handler will then call back into the registered callback in JS.
fn add_route_to_router(
  runtime_handle: RuntimeHandle,
  method: String,
  route: String,
) -> Result<Router<AppState>, AnyError> {
  let method_uppercase = method.to_uppercase();

  let route_path = route.clone();
  let handler = move |params: RawPathParams, user: Option<User>, req: Request| async move {
    let (parts, body) = req.into_parts();

    let Ok(body_bytes) = axum::body::to_bytes(body, usize::MAX).await else {
      return Err(JsHttpResponseError::Precondition(
        "request deserialization failed".to_string(),
      ));
    };
    let Parts { uri, headers, .. } = parts;

    let path_params: Vec<(String, String)> = params
      .iter()
      .map(|(k, v)| (k.to_string(), v.to_string()))
      .collect();
    let headers: Vec<(String, String)> = headers
      .into_iter()
      .filter_map(|(key, value)| {
        if let Some(key) = key {
          if let Ok(value) = value.to_str() {
            return Some((key.to_string(), value.to_string()));
          }
        }
        return None;
      })
      .collect();

    let js_user: Option<JsUser> = user.map(|u| JsUser {
      id: u.id,
      email: u.email,
      csrf: u.csrf_token,
    });

    let (sender, receiver) = oneshot::channel::<Result<JsHttpResponse, JsHttpResponseError>>();

    debug!("dispatch {method} {uri}");
    runtime_handle
      .send_to_any_isolate(build_http_dispatch_message(DispatchArgs {
        method,
        route_path,
        uri: uri.to_string(),
        path_params,
        headers,
        user: js_user,
        body: body_bytes,
        reply: sender,
      }))
      .await
      .map_err(|_err| JsHttpResponseError::Internal("send failed".into()))?;

    let js_response = receiver
      .await
      .map_err(|_err| JsHttpResponseError::Internal("receive failed".into()))??;

    let mut http_response = Response::builder()
      .status(js_response.status.unwrap_or(200))
      .body(Body::from(js_response.body.unwrap_or_default()))
      .map_err(|err| JsHttpResponseError::Internal(err.into()))?;

    if let Some(headers) = js_response.headers {
      for (key, value) in headers {
        http_response.headers_mut().insert(
          HeaderName::from_str(key.as_str())
            .map_err(|err| JsHttpResponseError::Internal(err.into()))?,
          HeaderValue::from_str(value.as_str())
            .map_err(|err| JsHttpResponseError::Internal(err.into()))?,
        );
      }
    }

    return Ok(http_response);
  };

  return Ok(Router::<AppState>::new().route(
    &route,
    match method_uppercase.as_str() {
      "DELETE" => axum::routing::delete(handler),
      "GET" => axum::routing::get(handler),
      "HEAD" => axum::routing::head(handler),
      "OPTIONS" => axum::routing::options(handler),
      "PATCH" => axum::routing::patch(handler),
      "POST" => axum::routing::post(handler),
      "PUT" => axum::routing::put(handler),
      "TRACE" => axum::routing::trace(handler),
      _ => {
        return Err(format!("method: {method_uppercase}").into());
      }
    },
  ));
}

async fn install_routes_and_jobs(
  state: &AppState,
  module: Module,
) -> Result<Option<Router<AppState>>, AnyError> {
  let runtime_handle = state.script_runtime();
  let jobs = state.jobs();

  // For all the isolates/worker-threads.
  let receivers: Vec<_> = runtime_handle
    .state()
    .iter()
    .enumerate()
    .map(async |(index, state)| {
      let module = module.clone();
      let runtime_handle = runtime_handle.clone();
      let jobs = jobs.clone();

      let (router_sender, router_receiver) = kanal::unbounded::<Router<AppState>>();

      if let Err(err) = state
        .send_privately(Message::Run(
          None,
          Box::new(move |_m, runtime: &mut Runtime, _c| {
            // First install a native callbacks.
            //
            // Register native callback for building axum router.
            let runtime_handle_clone = runtime_handle.clone();
            runtime
              .register_function("install_route", move |args: &[serde_json::Value]| {
                let method: String = get_arg(args, 0)?;
                let route: String = get_arg(args, 1)?;

                let router = add_route_to_router(runtime_handle_clone.clone(), method, route)
                  .map_err(|err| RSError::Runtime(err.to_string()))?;

                router_sender.send(router).expect("send");

                return Ok(serde_json::Value::Null);
              })
              .expect("Failed to register 'install_route' function");

            // Register native callback for registering cron jobs.
            runtime
              .register_function(
                "install_job",
                move |args: &[serde_json::Value]| -> Result<serde_json::Value, _> {
                  let name: String = get_arg(args, 0)?;
                  let default_spec: String = get_arg(args, 1)?;
                  let schedule = cron::Schedule::from_str(&default_spec).map_err(|err| {
                    return RSError::Runtime(err.to_string());
                  })?;

                  let runtime_handle = runtime_handle.clone();
                  let (id_sender, id_receiver) = oneshot::channel::<i64>();
                  let id_receiver = id_receiver.shared();

                  let Some(job) = jobs.new_job(
                    None,
                    name,
                    schedule,
                    crate::scheduler::build_callback(move || {
                      let runtime_handle = runtime_handle.clone();
                      let id_receiver = id_receiver.clone();

                      return async move {
                        let Some(first_isolate) = runtime_handle.state().first() else {
                          return Err("Missing isolate".into());
                        };

                        let id = id_receiver.await?;
                        first_isolate
                          .send_privately(build_call_async_js_function_message(
                            "cron".to_string(),
                            None,
                            "__dispatchCron",
                            [id],
                            move |value_or: Result<Option<String>, RSError>| {
                              match value_or {
                                Err(err) => debug!("cron failed: {err}"),
                                Ok(Some(err)) => debug!("cron failed: {err}"),
                                _ => {}
                              };
                            },
                          ))
                          .await?;

                        Ok::<_, AnyError>(())
                      };
                    }),
                  ) else {
                    return Err(RSError::Runtime("Failed to add job".to_string()));
                  };

                  if let Err(err) = id_sender.send(job.id as i64) {
                    return Err(RSError::Runtime(err.to_string()));
                  }

                  job.start();

                  return Ok(job.id.into());
                },
              )
              .expect("Failed to register 'install_job' function");
          }),
        ))
        .await
      {
        panic!("Failed to comm with v8 rt'{index}': {err}");
      }

      // Then execute the script/module, i.e. statements in the file scope.
      if let Err(err) = state.load_module(module).await {
        error!("Failed to load module: {err}");
        return None;
      }

      // Now all module-level calls to `install_route` should have happened. Let's drain the
      // registered routes. Note, we cannot `collect()` since the sender side never hangs up.
      let mut installed_routers: Vec<Router<AppState>> = vec![];
      match router_receiver.drain_into(&mut installed_routers) {
        Ok(n) => debug!("Got {n} routers from JS"),
        Err(err) => {
          error!("Failed to get routers from JS: {err}");
          return None;
        }
      };

      let mut merged_router = Router::<AppState>::new();
      for router in installed_routers {
        if router.has_routes() {
          merged_router = merged_router.merge(router);
        }
      }
      return Some(merged_router);
    })
    .collect();

  // Await function registration and module loading for all isolates/worker-threads.
  let mut receivers = futures_util::future::join_all(receivers).await;

  // Note: We only return the first router assuming that JS route registration is consistent across
  // all isolates.
  return Ok(receivers.swap_remove(0));
}

pub(crate) async fn load_routes_and_jobs_from_js_modules(
  state: &AppState,
) -> Result<Option<Router<AppState>>, AnyError> {
  let runtime_handle = state.script_runtime();
  if runtime_handle.num_threads() == 0 {
    info!("JS threads set to zero. Skipping initialization for JS modules");
    return Ok(None);
  }

  let scripts_dir = state.data_dir().root().join("scripts");
  let modules = match Module::load_dir(scripts_dir.clone()) {
    Ok(modules) => modules,
    Err(err) => {
      debug!("Skip loading js modules from '{scripts_dir:?}': {err}");
      return Ok(None);
    }
  };

  let mut js_router = Router::new();
  for module in modules {
    let fname = module.filename().to_owned();

    if let Some(router) = install_routes_and_jobs(state, module).await? {
      js_router = js_router.merge(router);
    } else {
      debug!("Skipping js module '{fname:?}': no routes");
    }
  }

  if js_router.has_routes() {
    return Ok(Some(js_router));
  }

  return Ok(None);
}
