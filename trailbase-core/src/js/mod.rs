use axum::extract::Request;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use axum::Router;
use parking_lot::Mutex;
use rustyscript::{json_args, Runtime};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use crate::AppState;

mod import_provider;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

const TRAILBASE_MAIN: &str = r#"
export const test = "test0";

const callbacks = new Map();

export function addRoute(method, route, callback) {
  rustyscript.functions.route(method, route);
  callbacks.set(`${method}:${route}`, callback);

  console.log("JS: Added route:", method, route);
}

globalThis.dispatch = (method, route, uri, headers, body) => {
  console.log("JS: Dispatching:", method, route, body);

  const key = `${method}:${route}`;
  const cb = callbacks.get(key);
  if (cb) {
    return cb({
      uri,
      headers,
      body,
    });
  }

  return `Missing callback: ${key}`;
};
"#;

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
      self.sender.send(Message::Close).unwrap();
      handle.join().unwrap();
    }
  }
}

fn init_runtime() -> Result<Runtime, AnyError> {
  let mut cache = import_provider::MemoryCache::default();

  cache.set("trailbase:main", TRAILBASE_MAIN.to_string());

  let runtime = rustyscript::Runtime::new(rustyscript::RuntimeOptions {
    import_provider: Some(Box::new(cache)),
    schema_whlist: HashSet::from(["trailbase".to_string()]),
    ..Default::default()
  })?;

  return Ok(runtime);
}

impl RuntimeSingleton {
  fn new() -> Self {
    let (sender, receiver) = crossbeam_channel::unbounded::<Message>();

    let handle = std::thread::spawn(move || {
      let mut runtime = init_runtime().unwrap();

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

    // TODO: remove.
    let _ = sender.send(Message::Run(Box::new(
      |runtime: &mut rustyscript::Runtime| {
        let module = rustyscript::Module::new(
          "trailbase:main",
          r#"
            import { test } from "trailbase:main";
            export const fun0 = test;
          "#,
        );
        let _handle = runtime.load_module(&module).unwrap();
      },
    )));

    return RuntimeSingleton {
      sender,
      handle: Some(handle),
    };
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

fn route_callback(
  state: AppState,
  router: Arc<Mutex<Option<Router<AppState>>>>,
  args: &[serde_json::Value],
) -> Result<(), rustyscript::Error> {
  let method: String = serde_json::from_value(args.first().unwrap().clone()).unwrap();
  let method_uppercase = method.to_uppercase();
  let route: String = serde_json::from_value(args.get(1).unwrap().clone()).unwrap();

  let route_path = route.clone();
  let handler = move |req: Request| async move {
    let (parts, body) = req.into_parts();

    let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
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

    let response = state
      .script_runtime()
      .apply(move |runtime| {
        let response: String = runtime
          .call_function(
            None,
            "dispatch",
            json_args!(method, route_path, uri.to_string(), headers, body_bytes),
          )
          .unwrap();
        return response;
      })
      .await
      .unwrap();

    return response.into_response();
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
        return Err(rustyscript::Error::ValueNotFound(format!(
          "method: {method_uppercase}"
        )));
      }
    },
  ));

  return Ok(());
}

pub(crate) async fn install_routes(
  state: AppState,
  script: rustyscript::Module,
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
            route_callback(state.clone(), router_clone.clone(), args)?;
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
              import { test } from "trailbase:main";

              export function test_fun() {
                return test;
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
