use axum::body::Body;
use axum::extract::Request;
use axum::http::{header::CONTENT_TYPE, request::Parts, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use parking_lot::Mutex;
use rustyscript::{json_args, Module, Runtime};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Arc, LazyLock};
use thiserror::Error;

use crate::AppState;

mod import_provider;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

const TRAILBASE_MAIN: &str = r#"
type Headers = { [key: string]: string };
type Request = {
  uri: string;
  headers: Headers;
  body: string;
};
type Response = {
  headers?: Headers;
  status?: number;
  body?: string;
};
type CbType = (req: Request) => Response | undefined;

const callbacks = new Map<string, CbType>();

export function addRoute(method: string, route: string, callback: CbType) {
  rustyscript.functions.route(method, route);
  callbacks.set(`${method}:${route}`, callback);

  console.log("JS: Added route:", method, route);
}

export function dispatch(
  method: string,
  route: string,
  uri: string,
  headers: Headers,
  body: string,
) : Response | undefined {
  console.log("JS: Dispatching:", method, route, body);

  const key = `${method}:${route}`;
  const cb = callbacks.get(key);
  if (!cb) {
    throw Error(`Missing callback: ${key}`);
  }

  return cb({
    uri,
    headers,
    body,
  });
};

globalThis.__dispatch = dispatch;
globalThis.__trailbase = {
  addRoute,
  dispatch,
};
"#;

#[derive(Default, Deserialize)]
struct JsResponse {
  headers: Option<HashMap<String, String>>,
  status: Option<u16>,
  body: Option<String>,
}

enum Message {
  Close,
  Run(Box<dyn (FnOnce(&mut Runtime)) + Send + Sync>),
}

struct RuntimeSingleton {
  sender: crossbeam_channel::Sender<Message>,
  handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for RuntimeSingleton {
  fn drop(&mut self) {
    if let Some(handle) = self.handle.take() {
      if self.sender.send(Message::Close).is_ok() {
        handle.join().unwrap();
      }
    }
  }
}

impl RuntimeSingleton {
  fn new() -> Self {
    let (sender, receiver) = crossbeam_channel::unbounded::<Message>();

    let handle = std::thread::spawn(move || {
      let mut runtime = Self::init_runtime().unwrap();

      let module = Module::new("__index.ts", TRAILBASE_MAIN);
      let _handle = runtime.load_module(&module).unwrap();

      #[allow(clippy::never_loop)]
      while let Ok(message) = receiver.recv() {
        match message {
          Message::Close => break,
          Message::Run(f) => {
            f(&mut runtime);
          }
        }
      }
    });

    return RuntimeSingleton {
      sender,
      handle: Some(handle),
    };
  }

  fn init_runtime() -> Result<Runtime, AnyError> {
    let mut cache = import_provider::MemoryCache::default();

    cache.set(
      "trailbase:main",
      r#"
        export const _test = "test0";
        export const addRoute = globalThis.__trailbase.addRoute;
      "#
      .to_string(),
    );

    let runtime = rustyscript::Runtime::new(rustyscript::RuntimeOptions {
      import_provider: Some(Box::new(cache)),
      schema_whlist: HashSet::from(["trailbase".to_string()]),
      ..Default::default()
    })?;

    return Ok(runtime);
  }
}

// NOTE: Repeated runtime initialization, e.g. in a multi-threaded context, leads to segfaults.
// rustyscript::init_platform is supposed to help with this but we haven't found a way to
// make it work. Thus, we're making the V8 VM a singleton (like Dart's).
static RUNTIME: LazyLock<RuntimeSingleton> = LazyLock::new(RuntimeSingleton::new);

pub(crate) struct RuntimeHandle;

impl RuntimeHandle {
  pub(crate) fn new() -> Self {
    return Self {};
  }

  #[allow(unused)]
  pub(crate) async fn apply<T>(
    &self,
    f: impl (FnOnce(&mut rustyscript::Runtime) -> T) + Send + Sync + 'static,
  ) -> Result<Box<T>, AnyError>
  where
    T: Send + Sync + 'static,
  {
    let (sender, mut receiver) = tokio::sync::oneshot::channel::<Box<T>>();

    RUNTIME.sender.send(Message::Run(Box::new(move |rt| {
      if let Err(_err) = sender.send(Box::new(f(rt))) {
        log::warn!("Failed to send");
      }
    })))?;

    return Ok(receiver.await?);
  }
}

#[derive(Debug, Error)]
pub enum JsResponseError {
  #[error("Precondition: {0}")]
  Precondition(String),
  #[error("Internal: {0}")]
  Internal(Box<dyn std::error::Error + Send + Sync>),
}

impl IntoResponse for JsResponseError {
  fn into_response(self) -> Response {
    let (status, body): (StatusCode, Option<String>) = match self {
      Self::Precondition(err) => (StatusCode::PRECONDITION_FAILED, Some(err.to_string())),
      Self::Internal(err) => (StatusCode::INTERNAL_SERVER_ERROR, Some(err.to_string())),
    };

    if let Some(body) = body {
      return Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "text/plain")
        .body(Body::new(body))
        .unwrap();
    }

    return Response::builder()
      .status(status)
      .body(Body::empty())
      .unwrap();
  }
}

/// Get's called from JS to `addRoute`.
fn route_callback(
  state: AppState,
  router: Arc<Mutex<Option<Router<AppState>>>>,
  args: &[serde_json::Value],
) -> Result<(), AnyError> {
  let Some(method) = args
    .first()
    .and_then(|v| serde_json::from_value::<String>(v.clone()).ok())
  else {
    return Err("Missing method argument".into());
  };
  let method_uppercase = method.to_uppercase();

  let Some(route) = args
    .get(1)
    .and_then(|v| serde_json::from_value::<String>(v.clone()).ok())
  else {
    return Err("Missing route argument".into());
  };

  let route_path = route.clone();
  let handler = move |req: Request| async move {
    let (parts, body) = req.into_parts();

    let Ok(body_bytes) = axum::body::to_bytes(body, usize::MAX).await else {
      return Err(JsResponseError::Precondition(
        "request deserialization failed".to_string(),
      ));
    };
    let Parts {
      method: _,
      uri,
      headers,
      ..
    } = parts;

    let headers: HashMap<String, String> = headers
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

    let js_response = state
      .script_runtime()
      .apply(move |runtime| -> Result<JsResponse, rustyscript::Error> {
        let response: JsResponse = runtime.call_function(
          None,
          "__dispatch",
          json_args!(
            method,
            route_path,
            uri.to_string(),
            headers,
            String::from_utf8_lossy(&body_bytes)
          ),
        )?;
        return Ok(response);
      })
      .await
      .map_err(JsResponseError::Internal)?
      .map_err(|err| JsResponseError::Internal(err.into()))?;

    let mut http_response = Response::builder()
      .status(js_response.status.unwrap_or(200))
      .body(Body::from(js_response.body.unwrap_or_default()))
      .map_err(|err| JsResponseError::Internal(err.into()))?;

    if let Some(headers) = js_response.headers {
      for (key, value) in headers {
        http_response.headers_mut().insert(
          HeaderName::from_str(key.as_str())
            .map_err(|err| JsResponseError::Internal(err.into()))?,
          HeaderValue::from_str(value.as_str())
            .map_err(|err| JsResponseError::Internal(err.into()))?,
        );
      }
    }

    return Ok(http_response);
  };

  let mut router = router.lock();
  *router = Some(router.take().unwrap().route(
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

  return Ok(());
}

pub(crate) async fn install_routes(
  state: AppState,
  script: Module,
) -> Result<Option<Router<AppState>>, AnyError> {
  return Ok(
    *state
      .clone()
      .script_runtime()
      .apply(move |runtime: &mut Runtime| {
        let router = Arc::new(Mutex::new(Some(Router::<AppState>::new())));

        // First install a native callback that builds an axum router.
        let router_clone = router.clone();
        runtime
          .register_function("route", move |args| {
            route_callback(state.clone(), router_clone.clone(), args)
              .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

            Ok(serde_json::Value::Null)
          })
          .unwrap();

        // Then execute the script, i.e. statements in the file scope.
        runtime.load_module(&script).unwrap();

        let router: Router<AppState> = router.lock().take().unwrap();
        if router.has_routes() {
          return Some(router);
        }
        return None;
      })
      .await?,
  );
}

#[cfg(test)]
mod tests {
  use super::*;
  use rustyscript::{json_args, Module};

  #[tokio::test]
  async fn test_runtime_apply() {
    let handle = RuntimeHandle::new();
    let number = handle
      .apply::<i64>(|_runtime| {
        return 42;
      })
      .await
      .unwrap();

    assert_eq!(42, *number);
  }

  #[tokio::test]
  async fn test_runtime_javascript() {
    let handle = RuntimeHandle::new();
    let result = handle
      .apply::<String>(|runtime| {
        let context = runtime
          .load_module(&Module::new(
            "module.js",
            r#"
              import { _test } from "trailbase:main";
              import { dispatch } from "./__index.ts";

              export function test_fun() {
                return _test;
              }
            "#,
          ))
          .map_err(|err| {
            log::error!("Failed to load module: {err}");
            return err;
          })
          .unwrap();

        return runtime
          .call_function(Some(&context), "test_fun", json_args!())
          .map_err(|err| {
            log::error!("Failed to load call fun: {err}");
            return err;
          })
          .unwrap();
      })
      .await
      .unwrap();

    assert_eq!("test0", *result);
  }
}
