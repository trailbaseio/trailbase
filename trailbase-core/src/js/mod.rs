use axum::body::Body;
use axum::extract::{RawPathParams, Request};
use axum::http::{header::CONTENT_TYPE, request::Parts, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use libsql::Connection;
use parking_lot::Mutex;
use rustyscript::{init_platform, json_args, Module, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::from_value;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{Arc, LazyLock};
use thiserror::Error;

use crate::assets::cow_to_string;
use crate::auth::user::User;
use crate::js::import_provider::JsRuntimeAssets;
use crate::records::sql_to_json::rows_to_json_arrays;
use crate::{AppState, DataDir};

mod import_provider;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

enum Message {
  Run(Box<dyn (FnOnce(&mut Runtime)) + Send + Sync>),
}

struct State {
  sender: crossbeam_channel::Sender<Message>,
  connection: Mutex<Option<libsql::Connection>>,
}

struct RuntimeSingleton {
  // Thread handle
  handle: Option<std::thread::JoinHandle<()>>,

  // Shared sender.
  sender: crossbeam_channel::Sender<Message>,

  // Isolate state.
  state: Vec<State>,
}

impl Drop for RuntimeSingleton {
  fn drop(&mut self) {
    if let Some(handle) = self.handle.take() {
      self.state.clear();
      if handle.join().is_err() {
        log::error!("Failed to join main rt thread");
      }
    }
  }
}

impl RuntimeSingleton {
  fn new() -> Self {
    let n_threads: usize = std::thread::available_parallelism().map_or_else(
      |err| {
        log::error!("Failed to get number of threads: {err}");
        return 1;
      },
      |x| x.get(),
    );

    log::info!("Starting v8 JavaScript runtime with {n_threads} workers.");

    let (shared_sender, shared_receiver) = crossbeam_channel::unbounded::<Message>();

    let (state, receivers): (Vec<State>, Vec<crossbeam_channel::Receiver<Message>>) = (0
      ..n_threads)
      .map(|_index| {
        let (sender, receiver) = crossbeam_channel::unbounded::<Message>();
        return (
          State {
            sender,
            connection: Mutex::new(None),
          },
          receiver,
        );
      })
      .unzip();

    let handle = std::thread::spawn(move || {
      init_platform(n_threads as u32, true);

      let threads: Vec<_> = receivers
        .into_iter()
        .enumerate()
        .map(|(index, receiver)| {
          let shared_receiver = shared_receiver.clone();

          return std::thread::spawn(move || {
            let mut runtime = match Self::init_runtime(index) {
              Ok(runtime) => runtime,
              Err(err) => {
                panic!("Failed to init v8 runtime on thread {index}: {err}");
              }
            };

            loop {
              crossbeam_channel::select! {
                recv(receiver) -> msg => {
                  match msg {
                    Ok(Message::Run(f)) => {
                      f(&mut runtime);
                    }
                    _ => {
                      log::info!("channel closed");
                      break;
                    }
                  }
                },
                recv(shared_receiver) -> msg => {
                  match msg {
                    Ok(Message::Run(f)) => {
                      f(&mut runtime);
                    }
                    _ => {
                      log::info!("shared channel closed");
                      break;
                    }
                  }
                },
              }
            }
          });
        })
        .collect();

      for thread in threads {
        if thread.join().is_err() {
          log::error!("Failed to join worker");
        }
      }
    });

    return RuntimeSingleton {
      sender: shared_sender,
      handle: Some(handle),
      state,
    };
  }

  fn init_runtime(index: usize) -> Result<Runtime, AnyError> {
    let tokio_runtime = tokio::runtime::Builder::new_current_thread()
      .enable_time()
      .enable_io()
      .thread_name("v8-runtime")
      .build()?;

    let mut runtime = rustyscript::Runtime::with_tokio_runtime(
      rustyscript::RuntimeOptions {
        import_provider: Some(Box::new(import_provider::ImportProviderImpl)),
        schema_whlist: HashSet::from(["trailbase".to_string()]),
        ..Default::default()
      },
      std::rc::Rc::new(tokio_runtime),
    )?;

    let idx = index;
    runtime.register_async_function("query", move |args: Vec<serde_json::Value>| {
      Box::pin(async move {
        let query: String = get_arg(&args, 0)?;
        let json_params: Vec<serde_json::Value> = get_arg(&args, 1)?;

        let mut params: Vec<libsql::Value> = vec![];
        for value in json_params {
          params.push(json_value_to_param(value)?);
        }

        let Some(conn) = RUNTIME.state[idx].connection.lock().clone() else {
          return Err(rustyscript::Error::Runtime(
            "missing db connection".to_string(),
          ));
        };

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
    })?;

    let idx = index;
    runtime.register_async_function("execute", move |args: Vec<serde_json::Value>| {
      Box::pin(async move {
        let query: String = get_arg(&args, 0)?;
        let json_params: Vec<serde_json::Value> = get_arg(&args, 1)?;

        let mut params: Vec<libsql::Value> = vec![];
        for value in json_params {
          params.push(json_value_to_param(value)?);
        }

        let Some(conn) = RUNTIME.state[idx].connection.lock().clone() else {
          return Err(rustyscript::Error::Runtime(
            "missing db connection".to_string(),
          ));
        };

        let rows_affected = conn
          .execute(&query, libsql::params::Params::Positional(params))
          .await
          .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

        return Ok(serde_json::Value::Number(rows_affected.into()));
      })
    })?;

    return Ok(runtime);
  }
}

// NOTE: Repeated runtime initialization, e.g. in a multi-threaded context, leads to segfaults.
// rustyscript::init_platform is supposed to help with this but we haven't found a way to
// make it work. Thus, we're making the V8 VM a singleton (like Dart's).
static RUNTIME: LazyLock<RuntimeSingleton> = LazyLock::new(RuntimeSingleton::new);

#[derive(Clone)]
pub(crate) struct RuntimeHandle;

impl RuntimeHandle {
  #[cfg(not(test))]
  pub(crate) fn set_connection(conn: Connection) {
    for s in &RUNTIME.state {
      let mut lock = s.connection.lock();
      if lock.is_some() {
        panic!("connection already set");
      }
      lock.replace(conn.clone());
    }
  }

  #[cfg(test)]
  pub(crate) fn set_connection(conn: Connection) {
    for s in &RUNTIME.state {
      let mut lock = s.connection.lock();
      if lock.is_some() {
        log::debug!("connection already set");
      } else {
        lock.replace(conn.clone());
      }
    }
  }

  #[cfg(test)]
  pub(crate) fn override_connection(conn: Connection) {
    for s in &RUNTIME.state {
      let mut lock = s.connection.lock();
      if lock.is_some() {
        log::debug!("connection already set");
      }
      lock.replace(conn.clone());
    }
  }

  pub(crate) fn new() -> Self {
    return Self {};
  }

  fn state(&self) -> &'static Vec<State> {
    return &RUNTIME.state;
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

/// Get's called from JS during `addRoute` and installs an axum HTTP handler.
///
/// The axum HTTP handler will then call back into the registered callback in JS.
fn add_route_to_router(
  runtime_handle: RuntimeHandle,
  router: Arc<Mutex<Option<Router<AppState>>>>,
  method: String,
  route: String,
) -> Result<(), AnyError> {
  let method_uppercase = method.to_uppercase();

  let route_path = route.clone();
  let handler = move |params: RawPathParams, user: Option<User>, req: Request| async move {
    let (parts, body) = req.into_parts();

    let Ok(body_bytes) = axum::body::to_bytes(body, usize::MAX).await else {
      return Err(JsResponseError::Precondition(
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

    #[derive(Serialize)]
    struct JsUser {
      // Base64 encoded user id.
      id: String,
      email: String,
      csrf: String,
    }

    let js_user: Option<JsUser> = user.map(|u| JsUser {
      id: u.id,
      email: u.email,
      csrf: u.csrf_token,
    });

    #[derive(Deserialize)]
    struct JsResponse {
      headers: Option<Vec<(String, String)>>,
      status: Option<u16>,
      body: Option<bytes::Bytes>,
    }

    let js_response = runtime_handle
      .apply(move |runtime| -> Result<JsResponse, rustyscript::Error> {
        let tokio_runtime = runtime.tokio_runtime();
        return tokio_runtime.block_on(async {
          return runtime
            .call_function_async::<JsResponse>(
              None,
              "__dispatch",
              json_args!(
                method,
                route_path,
                uri.to_string(),
                path_params,
                headers,
                js_user,
                body_bytes
              ),
            )
            .await;
        });
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
  runtime_handle: RuntimeHandle,
  module: Module,
) -> Result<Option<Router<AppState>>, AnyError> {
  use tokio::sync::oneshot;

  let receivers: Vec<_> = runtime_handle
    .state()
    .iter()
    .enumerate()
    .map(
      move |(index, state)| -> oneshot::Receiver<Option<Router<AppState>>> {
        let (sender, receiver) = oneshot::channel::<Option<Router<AppState>>>();

        let module = module.clone();
        let runtime_handle = runtime_handle.clone();

        if let Err(err) = state
          .sender
          .send(Message::Run(Box::new(move |runtime: &mut Runtime| {
            let router = Arc::new(Mutex::new(Some(Router::<AppState>::new())));

            // First install a native callback that builds an axum router.
            let router_clone = router.clone();
            runtime
              .register_function("route", move |args: &[serde_json::Value]| {
                let method: String = get_arg(args, 0)?;
                let route: String = get_arg(args, 1)?;

                add_route_to_router(runtime_handle.clone(), router_clone.clone(), method, route)
                  .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

                Ok(serde_json::Value::Null)
              })
              .expect("Failed to register 'route' function");

            // Then execute the script/module, i.e. statements in the file scope.
            //
            // TODO: SWC is very spammy (at least in debug builds). Ideally, we'd lower the tracing
            // filter level within this scope. Haven't found a good way, thus filtering it
            // env-filter at the CLI level. We could try to use a dedicated reload layer:
            //   https://docs.rs/tracing-subscriber/latest/tracing_subscriber/reload/index.html
            if let Err(err) = runtime.load_module(&module) {
              panic!("Failed to load '{:?}': {err}", module.filename());
            }

            let router: Router<AppState> = router.lock().take().unwrap();
            sender
              .send(if router.has_routes() {
                Some(router)
              } else {
                None
              })
              .expect("Failed to comm with parent");
          })))
        {
          panic!("Failed to comm with v8 rt'{index}': {err}");
        }

        return receiver;
      },
    )
    .collect();

  let mut receivers = futures::future::join_all(receivers).await;

  // Note: We only return the first router assuming that js route registration is deterministic.
  return Ok(receivers.swap_remove(0)?);
}

pub(crate) async fn write_js_runtime_files(data_dir: &DataDir) {
  if let Err(err) = tokio::fs::write(
    data_dir.root().join("trailbase.js"),
    cow_to_string(
      JsRuntimeAssets::get("index.js")
        .expect("Failed to read rt/index.js")
        .data,
    )
    .as_str(),
  )
  .await
  {
    log::warn!("Failed to write 'trailbase.js': {err}");
  }

  if let Err(err) = tokio::fs::write(
    data_dir.root().join("trailbase.d.ts"),
    cow_to_string(
      JsRuntimeAssets::get("index.d.ts")
        .expect("Failed to read rt/index.d.ts")
        .data,
    )
    .as_str(),
  )
  .await
  {
    log::warn!("Failed to write 'trailbase.d.ts': {err}");
  }
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
    let handle = RuntimeHandle::new();
    let number = handle
      .apply::<i64>(|_runtime| {
        return 42;
      })
      .await
      .unwrap();

    assert_eq!(42, *number);
  }

  async fn test_runtime_javascript() {
    let handle = RuntimeHandle::new();
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
            log::error!("Failed to load call test_fun: {err}");
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

    RuntimeHandle::override_connection(conn);
    let handle = RuntimeHandle::new();

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
            log::error!("Failed to load call test_query: {err}");
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

    RuntimeHandle::override_connection(conn.clone());
    let handle = RuntimeHandle::new();

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
            log::error!("Failed to load call test_execute: {err}");
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
