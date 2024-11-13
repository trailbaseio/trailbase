use axum::body::Body;
use axum::extract::Request;
use axum::http::{header::CONTENT_TYPE, request::Parts, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use libsql::Connection;
use parking_lot::Mutex;
use rust_embed::RustEmbed;
use rustyscript::{json_args, Module, Runtime};
use serde::Deserialize;
use serde_json::from_value;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Arc, LazyLock};
use thiserror::Error;

use crate::assets::cow_to_string;
use crate::records::sql_to_json::rows_to_json_arrays;
use crate::AppState;

mod import_provider;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

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
      cow_to_string(JsRuntimeAssets::get("index.js").unwrap().data),
    );

    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
      .enable_time()
      .enable_io()
      .thread_name("v8-runtime")
      .thread_stack_size(4 * 1024 * 1024)
      .build()?;

    let runtime = rustyscript::Runtime::with_tokio_runtime(
      rustyscript::RuntimeOptions {
        import_provider: Some(Box::new(cache)),
        schema_whlist: HashSet::from(["trailbase".to_string()]),
        ..Default::default()
      },
      std::rc::Rc::new(tokio_runtime),
    )?;

    return Ok(runtime);
  }
}

// NOTE: Repeated runtime initialization, e.g. in a multi-threaded context, leads to segfaults.
// rustyscript::init_platform is supposed to help with this but we haven't found a way to
// make it work. Thus, we're making the V8 VM a singleton (like Dart's).
static RUNTIME: LazyLock<RuntimeSingleton> = LazyLock::new(RuntimeSingleton::new);

pub(crate) struct RuntimeHandle;

pub fn json_value_to_param(value: serde_json::Value) -> Result<libsql::Value, rustyscript::Error> {
  use rustyscript::Error;
  return Ok(match value {
    serde_json::Value::Object(ref _map) => {
      return Err(Error::Runtime("Object unsupported".to_string()));
    }
    serde_json::Value::Array(ref _arr) => {
      return Err(Error::Runtime("Array unsupported".to_string()));
    }
    serde_json::Value::Null => libsql::Value::Null,
    serde_json::Value::Bool(b) => libsql::Value::Integer(b as i64),
    serde_json::Value::String(str) => libsql::Value::Text(str),
    serde_json::Value::Number(number) => {
      if let Some(n) = number.as_i64() {
        libsql::Value::Integer(n)
      } else if let Some(n) = number.as_u64() {
        libsql::Value::Integer(n as i64)
      } else if let Some(n) = number.as_f64() {
        libsql::Value::Real(n)
      } else {
        return Err(Error::Runtime(format!("invalid number: {number:?}")));
      }
    }
  });
}

impl RuntimeHandle {
  pub(crate) fn new(conn: Connection) -> Self {
    RUNTIME
      .sender
      .send(Message::Run(Box::new(move |runtime: &mut Runtime| {
        let conn_clone = conn.clone();

        runtime
          .register_async_function("query", move |args: Vec<serde_json::Value>| {
            let conn = conn_clone.clone();
            Box::pin(async move {
              let query: String = get_arg(&args, 0)?;
              let json_params: Vec<serde_json::Value> = get_arg(&args, 1)?;

              let mut params: Vec<libsql::Value> = vec![];
              for value in json_params {
                params.push(json_value_to_param(value)?);
              }

              let rows = conn
                .query(&query, libsql::params::Params::Positional(params))
                .await
                .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

              let (values, _columns) = rows_to_json_arrays(rows, usize::MAX)
                .await
                .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

              return serde_json::to_value(values)
                .map_err(|err| rustyscript::Error::Runtime(err.to_string()));
            })
          })
          .unwrap();

        runtime
          .register_async_function("execute", move |args: Vec<serde_json::Value>| {
            let conn = conn.clone();
            Box::pin(async move {
              let query: String = get_arg(&args, 0)?;
              let json_params: Vec<serde_json::Value> = get_arg(&args, 1)?;

              let mut params: Vec<libsql::Value> = vec![];
              for value in json_params {
                params.push(json_value_to_param(value)?);
              }

              let rows_affected = conn
                .execute(&query, libsql::params::Params::Positional(params))
                .await
                .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

              return Ok(serde_json::Value::Number(rows_affected.into()));
            })
          })
          .unwrap();
      })))
      .unwrap();

    return Self {};
  }

  async fn apply<T>(
    &self,
    f: impl (FnOnce(&mut rustyscript::Runtime) -> T) + Send + Sync + 'static,
  ) -> Result<Box<T>, AnyError>
  where
    T: Send + Sync + 'static,
  {
    let (sender, receiver) = tokio::sync::oneshot::channel::<Box<T>>();

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
  method: String,
  route: String,
) -> Result<(), AnyError> {
  let method_uppercase = method.to_uppercase();
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

fn get_arg<T>(args: &[serde_json::Value], i: usize) -> Result<T, rustyscript::Error>
where
  T: serde::de::DeserializeOwned,
{
  use rustyscript::Error;
  let arg = args
    .get(i)
    .ok_or_else(|| Error::Runtime(format!("Range err {i} > {}", args.len())))?;
  return from_value::<T>(arg.clone()).map_err(|err| Error::Runtime(err.to_string()));
}

pub(crate) async fn install_routes(
  state: AppState,
  module: Module,
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
          .register_function("route", move |args: &[serde_json::Value]| {
            let method: String = get_arg(args, 0)?;
            let route: String = get_arg(args, 1)?;

            route_callback(state.clone(), router_clone.clone(), method, route)
              .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

            Ok(serde_json::Value::Null)
          })
          .unwrap();

        // Then execute the script/module, i.e. statements in the file scope.
        runtime.load_module(&module).unwrap();

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
  use trailbase_sqlite::query_one_row;

  async fn new_mem_conn() -> libsql::Connection {
    return libsql::Builder::new_local(":memory:")
      .build()
      .await
      .unwrap()
      .connect()
      .unwrap();
  }

  #[tokio::test]
  async fn test_serial_tests() {
    // NOTE: needs to run serially since registration of libsql connection with singleton v8 runtime
    // is racy.
    test_runtime_apply().await;
    test_runtime_javascript().await;
    test_javascript_query().await;
    test_javascript_execute().await;
  }

  async fn test_runtime_apply() {
    let handle = RuntimeHandle::new(new_mem_conn().await);
    let number = handle
      .apply::<i64>(|_runtime| {
        return 42;
      })
      .await
      .unwrap();

    assert_eq!(42, *number);
  }

  async fn test_runtime_javascript() {
    let handle = RuntimeHandle::new(new_mem_conn().await);
    let result = handle
      .apply::<String>(|runtime| {
        let context = runtime
          .load_module(&Module::new(
            "module.js",
            r#"
              export function test_fun() {
                return "test0";
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

  async fn test_javascript_query() {
    let conn = new_mem_conn().await;
    conn
      .execute("CREATE TABLE test (v0 TEXT, v1 INTEGER);", ())
      .await
      .unwrap();
    conn
      .execute("INSERT INTO test (v0, v1) VALUES ('0', 0), ('1', 1);", ())
      .await
      .unwrap();

    let handle = RuntimeHandle::new(conn);

    let result = handle
      .apply::<Vec<Vec<serde_json::Value>>>(|runtime| {
        let context = runtime
          .load_module(&Module::new(
            "module.ts",
            r#"
              import { query } from "trailbase:main";

              export async function test_query(queryStr: string) : Promise<unknown[][]> {
                return await query(queryStr, []);
              }
            "#,
          ))
          .map_err(|err| {
            log::error!("Failed to load module: {err}");
            return err;
          })
          .unwrap();

        let tokio_runtime = runtime.tokio_runtime();
        return tokio_runtime
          .block_on(async {
            runtime
              .call_function_async(
                Some(&context),
                "test_query",
                json_args!("SELECT * FROM test"),
              )
              .await
          })
          .map_err(|err| {
            log::error!("Failed to load call fun: {err}");
            return err;
          })
          .unwrap();
      })
      .await
      .unwrap();

    assert_eq!(
      vec![
        vec![
          serde_json::Value::String("0".to_string()),
          serde_json::Value::Number(0.into())
        ],
        vec![
          serde_json::Value::String("1".to_string()),
          serde_json::Value::Number(1.into())
        ],
      ],
      *result
    );
  }

  async fn test_javascript_execute() {
    let conn = new_mem_conn().await;
    conn
      .execute("CREATE TABLE test (v0 TEXT, v1 INTEGER);", ())
      .await
      .unwrap();

    let handle = RuntimeHandle::new(conn.clone());

    let _result = handle
      .apply::<i64>(|runtime| {
        let context = runtime
          .load_module(&Module::new(
            "module.ts",
            r#"
              import { execute } from "trailbase:main";

              export async function test_execute(queryStr: string) : Promise<number> {
                return await execute(queryStr, []);
              }
            "#,
          ))
          .map_err(|err| {
            log::error!("Failed to load module: {err}");
            return err;
          })
          .unwrap();

        let tokio_runtime = runtime.tokio_runtime();
        return tokio_runtime
          .block_on(async {
            runtime
              .call_function_async(
                Some(&context),
                "test_execute",
                json_args!("DELETE FROM test"),
              )
              .await
          })
          .map_err(|err| {
            log::error!("Failed to load call fun: {err}");
            return err;
          })
          .unwrap();
      })
      .await
      .unwrap();

    let row = query_one_row(&conn, "SELECT COUNT(*) FROM test", ())
      .await
      .unwrap();
    let count: i64 = row.get(0).unwrap();
    assert_eq!(0, count);
  }
}

#[derive(RustEmbed, Clone)]
#[folder = "js/dist/"]
struct JsRuntimeAssets;
