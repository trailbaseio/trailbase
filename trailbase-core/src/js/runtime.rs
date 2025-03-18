use axum::body::Body;
use axum::extract::{RawPathParams, Request};
use axum::http::{header::CONTENT_TYPE, request::Parts, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use parking_lot::Mutex;
use rustyscript::{
  deno_core::PollEventLoopOptions, init_platform, js_value::Promise, json_args, Error as RSError,
  Module, Runtime,
};
use serde::{Deserialize, Serialize};
use serde_json::from_value;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::oneshot;

use crate::assets::cow_to_string;
use crate::auth::user::User;
use crate::js::import_provider::JsRuntimeAssets;
use crate::records::sql_to_json::rows_to_json_arrays;
use crate::{AppState, DataDir};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Deserialize, Default, Debug)]
struct JsResponse {
  headers: Option<Vec<(String, String)>>,
  status: Option<u16>,
  body: Option<bytes::Bytes>,
}

#[derive(Debug, Error)]
pub enum JsResponseError {
  #[error("Precondition: {0}")]
  Precondition(String),
  #[error("Internal: {0}")]
  Internal(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Serialize)]
struct JsUser {
  // Base64 encoded user id.
  id: String,
  email: String,
  csrf: String,
}

struct DispatchArgs {
  method: String,
  route_path: String,
  uri: String,
  path_params: Vec<(String, String)>,
  headers: Vec<(String, String)>,
  user: Option<JsUser>,
  body: bytes::Bytes,

  reply: tokio::sync::oneshot::Sender<Result<JsResponse, JsResponseError>>,
}

enum Message {
  Run(Box<dyn (FnOnce(&mut Runtime)) + Send + Sync>),
  Dispatch(DispatchArgs),
  CallFunction(
    Option<Module>,
    &'static str,
    Vec<serde_json::Value>,
    tokio::sync::oneshot::Sender<Result<serde_json::Value, AnyError>>,
  ),
  LoadModule(Module, tokio::sync::oneshot::Sender<Result<(), AnyError>>),
}

struct State {
  sender: async_channel::Sender<Message>,
  connection: Mutex<Option<trailbase_sqlite::Connection>>,
}

struct RuntimeSingleton {
  n_threads: usize,

  // Thread handle
  handle: Option<std::thread::JoinHandle<()>>,

  // Shared sender.
  sender: async_channel::Sender<Message>,

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

struct Completer {
  name: String,
  promise: Promise<JsResponse>,
  reply: tokio::sync::oneshot::Sender<Result<JsResponse, JsResponseError>>,
}

impl Completer {
  fn is_ready(&self, runtime: &mut Runtime) -> bool {
    return !self.promise.is_pending(runtime);
  }

  async fn resolve(self, runtime: &mut Runtime) {
    let value = self
      .promise
      .into_future(runtime)
      .await
      .map_err(|err| JsResponseError::Internal(err.into()));

    if self.reply.send(value).is_err() {
      log::error!("Completer send failed for : {}", self.name);
    }
  }
}

impl RuntimeSingleton {
  async fn handle_message(
    runtime: &mut Runtime,
    msg: Message,
    completers: &mut Vec<Completer>,
  ) -> Result<(), AnyError> {
    match msg {
      Message::Run(f) => {
        f(runtime);
      }
      Message::Dispatch(args) => {
        let channel = args.reply;
        let uri = args.uri.clone();
        let promise = match runtime.call_function_immediate::<Promise<JsResponse>>(
          None,
          "__dispatch",
          json_args!(
            args.method,
            args.route_path,
            args.uri,
            args.path_params,
            args.headers,
            args.user,
            args.body
          ),
        ) {
          Ok(promise) => promise,
          Err(err) => {
            if channel
              .send(Err(JsResponseError::Internal(err.into())))
              .is_err()
            {
              log::error!("dispatch sending error failed");
            }
            return Ok(());
          }
        };

        completers.push(Completer {
          name: uri,
          promise,
          reply: channel,
        });
      }
      Message::CallFunction(module, name, args, sender) => {
        let module_handle = if let Some(module) = module {
          runtime.load_module_async(&module).await.ok()
        } else {
          None
        };

        let result: Result<serde_json::Value, AnyError> = runtime
          .call_function_async::<serde_json::Value>(module_handle.as_ref(), name, &args)
          .await
          .map_err(|err| err.into());

        if sender.send(result).is_err() {
          log::error!("Sending of js function call reply failed");
        }
      }
      Message::LoadModule(module, sender) => {
        runtime.load_module_async(&module).await?;
        if sender.send(Ok(())).is_err() {
          log::error!("Load module send failed");
        }
      }
    }

    return Ok(());
  }

  fn event_loop(
    runtime: &mut Runtime,
    private_recv: async_channel::Receiver<Message>,
    shared_recv: async_channel::Receiver<Message>,
  ) {
    runtime.tokio_runtime().block_on(async {
      let mut completers: Vec<Completer> = vec![];

      loop {
        let completed = completers
          .iter()
          .enumerate()
          .filter_map(|(idx, completer)| {
            if completer.is_ready(runtime) {
              Some(idx)
            } else {
              None
            }
          })
          .collect::<Vec<_>>();

        for index in completed.into_iter().rev() {
          let completer = completers.swap_remove(index);
          completer.resolve(runtime).await;
        }
        let pending = !completers.is_empty();

        const DURATION: Option<Duration> = Some(Duration::from_millis(25));
        const OPTS: PollEventLoopOptions = PollEventLoopOptions {
          wait_for_inspector: false,
          pump_v8_message_loop: true,
        };

        tokio::select! {
          result = runtime.await_event_loop(OPTS, DURATION), if pending => {
            if let Err(err) = result{
              log::error!("JS event loop: {err}");
            }
          },
          msg = private_recv.recv() => {
            let Ok(msg) = msg else {
              panic!("private channel closed");
            };
            if let Err(err) = Self::handle_message(runtime, msg, &mut completers).await {
              log::error!("Handle private message: {err}");
            }
          },
          msg = shared_recv.recv() => {
            let Ok(msg) = msg else {
              panic!("private channel closed");
            };
            if let Err(err) = Self::handle_message(runtime, msg, &mut completers).await {
              log::error!("Handle shared message: {err}");
            }
          },
        }
      }
    });
  }

  fn new_with_threads(threads: Option<usize>) -> Self {
    let n_threads = match threads {
      Some(n) => n,
      None => std::thread::available_parallelism().map_or_else(
        |err| {
          log::error!("Failed to get number of threads: {err}");
          return 1;
        },
        |x| x.get(),
      ),
    };

    log::info!("Starting v8 JavaScript runtime with {n_threads} workers.");

    let (shared_sender, shared_receiver) = async_channel::unbounded::<Message>();

    let (state, receivers): (Vec<State>, Vec<async_channel::Receiver<Message>>) = (0..n_threads)
      .map(|_index| {
        let (sender, receiver) = async_channel::unbounded::<Message>();

        return (
          State {
            sender,
            connection: Mutex::new(None),
          },
          receiver,
        );
      })
      .unzip();

    let handle = if n_threads > 0 {
      Some(std::thread::spawn(move || {
        init_platform(n_threads as u32, true);

        let threads: Vec<_> = receivers
          .into_iter()
          .enumerate()
          .map(|(index, receiver)| {
            let shared_receiver = shared_receiver.clone();

            return std::thread::spawn(move || {
              let tokio_runtime = std::rc::Rc::new(
                tokio::runtime::Builder::new_current_thread()
                  .enable_time()
                  .enable_io()
                  .thread_name("v8-runtime")
                  .build()
                  .expect("startup"),
              );

              let mut js_runtime = match Self::init_runtime(index, tokio_runtime.clone()) {
                Ok(js_runtime) => js_runtime,
                Err(err) => {
                  panic!("Failed to init v8 runtime on thread {index}: {err}");
                }
              };

              Self::event_loop(&mut js_runtime, receiver, shared_receiver);
            });
          })
          .collect();

        for (idx, thread) in threads.into_iter().enumerate() {
          if let Err(err) = thread.join() {
            log::error!("Failed to join worker: {idx}: {err:?}");
          }
        }
      }))
    } else {
      None
    };

    return RuntimeSingleton {
      n_threads,
      sender: shared_sender,
      handle,
      state,
    };
  }

  fn init_runtime(
    index: usize,
    tokio_runtime: std::rc::Rc<tokio::runtime::Runtime>,
  ) -> Result<Runtime, AnyError> {
    let mut runtime = rustyscript::Runtime::with_tokio_runtime(
      rustyscript::RuntimeOptions {
        import_provider: Some(Box::new(crate::js::import_provider::ImportProviderImpl)),
        schema_whlist: HashSet::from(["trailbase".to_string()]),
        ..Default::default()
      },
      tokio_runtime,
    )?;

    let idx = index;
    runtime
      .register_function("isolate_id", move |_args: &[serde_json::Value]| {
        return Ok(serde_json::json!(idx));
      })
      .expect("Failed to register 'isolate_id' function");

    let idx = index;
    runtime.register_async_function("query", move |args: Vec<serde_json::Value>| {
      Box::pin(async move {
        let query: String = get_arg(&args, 0)?;
        let json_params: Vec<serde_json::Value> = get_arg(&args, 1)?;

        let mut params: Vec<trailbase_sqlite::Value> = vec![];
        for value in json_params {
          params.push(json_value_to_param(value)?);
        }

        let Some(conn) = get_runtime(None).state[idx].connection.lock().clone() else {
          return Err(rustyscript::Error::Runtime(
            "missing db connection".to_string(),
          ));
        };

        let rows = conn
          .query(&query, params)
          .await
          .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

        let (values, _columns) = rows_to_json_arrays(rows, usize::MAX)
          .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

        return Ok(serde_json::json!(values));
      })
    })?;

    let idx = index;
    runtime.register_async_function("execute", move |args: Vec<serde_json::Value>| {
      Box::pin(async move {
        let query: String = get_arg(&args, 0)?;
        let json_params: Vec<serde_json::Value> = get_arg(&args, 1)?;

        let mut params: Vec<trailbase_sqlite::Value> = vec![];
        for value in json_params {
          params.push(json_value_to_param(value)?);
        }

        let Some(conn) = get_runtime(None).state[idx].connection.lock().clone() else {
          return Err(rustyscript::Error::Runtime(
            "missing db connection".to_string(),
          ));
        };

        let rows_affected = conn
          .execute(&query, params)
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
fn get_runtime(n_threads: Option<usize>) -> &'static RuntimeSingleton {
  static RUNTIME: OnceLock<RuntimeSingleton> = OnceLock::new();
  return RUNTIME.get_or_init(move || RuntimeSingleton::new_with_threads(n_threads));
}

#[derive(Clone)]
pub(crate) struct RuntimeHandle {
  runtime: &'static RuntimeSingleton,
}

impl RuntimeHandle {
  #[cfg(not(test))]
  pub(crate) fn set_connection(&self, conn: trailbase_sqlite::Connection) {
    for s in &self.runtime.state {
      let mut lock = s.connection.lock();
      if lock.is_some() {
        panic!("connection already set");
      }
      lock.replace(conn.clone());
    }
  }

  #[cfg(test)]
  pub(crate) fn set_connection(&self, conn: trailbase_sqlite::Connection) {
    for s in &self.runtime.state {
      let mut lock = s.connection.lock();
      if lock.is_some() {
        log::debug!("connection already set");
      } else {
        lock.replace(conn.clone());
      }
    }
  }

  #[cfg(test)]
  pub(crate) fn override_connection(&self, conn: trailbase_sqlite::Connection) {
    for s in &self.runtime.state {
      let mut lock = s.connection.lock();
      if lock.is_some() {
        log::debug!("connection already set");
      }
      lock.replace(conn.clone());
    }
  }

  pub(crate) fn new() -> Self {
    return Self {
      runtime: get_runtime(None),
    };
  }

  pub(crate) fn new_with_threads(n_threads: usize) -> Self {
    return Self {
      runtime: get_runtime(Some(n_threads)),
    };
  }

  fn state(&self) -> &'static Vec<State> {
    return &self.runtime.state;
  }

  async fn call_function<T>(
    &self,
    module: Option<Module>,
    name: &'static str,
    args: Vec<serde_json::Value>,
  ) -> Result<T, AnyError>
  where
    T: serde::de::DeserializeOwned,
  {
    let (sender, receiver) = tokio::sync::oneshot::channel::<Result<serde_json::Value, AnyError>>();
    self
      .runtime
      .sender
      .send(Message::CallFunction(module, name, args, sender))
      .await?;

    return Ok(serde_json::from_value::<T>(receiver.await??)?);
  }
}

pub fn json_value_to_param(
  value: serde_json::Value,
) -> Result<trailbase_sqlite::Value, rustyscript::Error> {
  use rustyscript::Error;
  return Ok(match value {
    serde_json::Value::Object(ref _map) => {
      return Err(Error::Runtime("Object unsupported".to_string()));
    }
    serde_json::Value::Array(ref _arr) => {
      return Err(Error::Runtime("Array unsupported".to_string()));
    }
    serde_json::Value::Null => trailbase_sqlite::Value::Null,
    serde_json::Value::Bool(b) => trailbase_sqlite::Value::Integer(b as i64),
    serde_json::Value::String(str) => trailbase_sqlite::Value::Text(str),
    serde_json::Value::Number(number) => {
      if let Some(n) = number.as_i64() {
        trailbase_sqlite::Value::Integer(n)
      } else if let Some(n) = number.as_u64() {
        trailbase_sqlite::Value::Integer(n as i64)
      } else if let Some(n) = number.as_f64() {
        trailbase_sqlite::Value::Real(n)
      } else {
        return Err(Error::Runtime(format!("invalid number: {number:?}")));
      }
    }
  });
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
        .unwrap_or_default();
    }

    return Response::builder()
      .status(status)
      .body(Body::empty())
      .unwrap_or_default();
  }
}

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

    let js_user: Option<JsUser> = user.map(|u| JsUser {
      id: u.id,
      email: u.email,
      csrf: u.csrf_token,
    });

    let (sender, receiver) = tokio::sync::oneshot::channel::<Result<JsResponse, JsResponseError>>();

    log::debug!("dispatch {method} {uri}");
    runtime_handle
      .runtime
      .sender
      .send(Message::Dispatch(DispatchArgs {
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
      .map_err(|_err| JsResponseError::Internal("send failed".into()))?;

    let js_response = receiver
      .await
      .map_err(|_err| JsResponseError::Internal("receive failed".into()))??;

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

async fn install_routes_and_jobs(
  state: &AppState,
  module: Module,
) -> Result<Option<Router<AppState>>, AnyError> {
  let runtime_handle = state.script_runtime();
  let jobs = state.jobs();

  let receivers: Vec<_> = runtime_handle
    .state()
    .iter()
    .enumerate()
    .map(move |(index, state)| {
      let module = module.clone();
      let runtime_handle = runtime_handle.clone();
      let jobs = jobs.clone();

      async move {
        let routers = Arc::new(Mutex::new(Vec::new()));

        let runtime_handle_clone = runtime_handle.clone();
        let routers_clone = routers.clone();
        if let Err(err) = state
          .sender
          .send(Message::Run(Box::new(move |runtime: &mut Runtime| {
            // First install a native callbacks.
            //
            // Register native callback for building axum router.
            runtime
              .register_function("install_route", move |args: &[serde_json::Value]| {
                let method: String = get_arg(args, 0)?;
                let route: String = get_arg(args, 1)?;

                let router = add_route_to_router(runtime_handle_clone.clone(), method, route)
                  .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

                routers_clone.lock().push(router);

                return Ok(serde_json::Value::Null);
              })
              .expect("Failed to register 'install_route' function");

            // Register native callback for registering cron jobs.
            runtime
              .register_function("install_job", move |args: &[serde_json::Value]| {
                let id: i32 = get_arg(args, 0)?;
                let id_value = serde_json::to_value(id)?;

                let name: String = get_arg(args, 1)?;
                let default_spec: String = get_arg(args, 2)?;
                let schedule = cron::Schedule::from_str(&default_spec).map_err(|err| {
                  return RSError::Runtime(err.to_string());
                })?;

                let runtime_handle = runtime_handle.clone();
                let Some(job) = jobs.new_job(
                  Some(id),
                  name,
                  schedule,
                  crate::scheduler::build_callback(move || {
                    let runtime_handle = runtime_handle.clone();
                    let id_value = id_value.clone();

                    return async move {
                      log::debug!("dispatch job: {id}");

                      if let Some(msg) = runtime_handle
                        .call_function::<Option<String>>(None, "__dispatchCron", vec![id_value])
                        .await?
                      {
                        return Err(msg.into());
                      }

                      Ok::<_, AnyError>(())
                    };
                  }),
                ) else {
                  return Err(RSError::Runtime("Failed to add job".to_string()));
                };

                job.start();

                return Ok(serde_json::Value::Null);
              })
              .expect("Failed to register 'install_job' function");
          })))
          .await
        {
          panic!("Failed to comm with v8 rt'{index}': {err}");
        }

        // Then execute the script/module, i.e. statements in the file scope.
        match await_loading_module(state, module).await {
          Ok(()) => {
            let mut merged_router = Router::<AppState>::new();
            for router in routers.lock().split_off(0) {
              merged_router = merged_router.merge(router);
            }

            if merged_router.has_routes() {
              Some(merged_router)
            } else {
              None
            }
          }
          Err(err) => {
            log::error!("Failed to load module: {err}");
            None
          }
        }
      }
    })
    .collect();

  let mut receivers = futures_util::future::join_all(receivers).await;

  // Note: We only return the first router assuming that js route registration is consistent across
  // all isolates.
  return Ok(receivers.swap_remove(0));
}

async fn await_loading_module(state: &State, module: Module) -> Result<(), AnyError> {
  let (sender, receiver) = oneshot::channel::<Result<(), AnyError>>();

  state
    .sender
    .send(Message::LoadModule(module, sender))
    .await?;

  let _ = receiver.await.map_err(|err| {
    log::error!("Failed to await module loading: {err}");
    return err;
  })?;

  return Ok(());
}

pub(crate) async fn load_routes_and_jobs_from_js_modules(
  state: &AppState,
) -> Result<Option<Router<AppState>>, AnyError> {
  let runtime_handle = state.script_runtime();
  if runtime_handle.runtime.n_threads == 0 {
    log::info!("JS threads set to zero. Skipping initialization for JS modules");
    return Ok(None);
  }

  let scripts_dir = state.data_dir().root().join("scripts");

  let modules = match rustyscript::Module::load_dir(scripts_dir.clone()) {
    Ok(modules) => modules,
    Err(err) => {
      log::debug!("Skip loading js modules from '{scripts_dir:?}': {err}");
      return Ok(None);
    }
  };

  let mut js_router = Router::new();
  for module in modules {
    let fname = module.filename().to_owned();

    if let Some(router) = install_routes_and_jobs(state, module).await? {
      js_router = js_router.merge(router);
    } else {
      log::debug!("Skipping js module '{fname:?}': no routes");
    }
  }

  if js_router.has_routes() {
    return Ok(Some(js_router));
  }

  return Ok(None);
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
  use rustyscript::Module;

  #[tokio::test]
  async fn test_serial_tests() {
    // NOTE: needs to run serially since registration of SQLite connection with singleton v8
    // runtime is racy.
    test_runtime_apply().await;
    test_runtime_javascript().await;
    test_javascript_query().await;
    test_javascript_execute().await;
  }

  async fn test_runtime_apply() {
    let (sender, receiver) = tokio::sync::oneshot::channel::<i64>();

    let handle = RuntimeHandle::new();
    handle
      .runtime
      .sender
      .send(Message::Run(Box::new(|_rt| {
        sender.send(5).unwrap();
      })))
      .await
      .unwrap();

    assert_eq!(5, receiver.await.unwrap());
  }

  async fn test_runtime_javascript() {
    let handle = RuntimeHandle::new();

    let module = Module::new(
      "module.js",
      r#"
              export function test_fun() {
                return "test0";
              }
            "#,
    );

    let result = handle
      .call_function::<String>(Some(module), "test_fun", vec![])
      .await
      .unwrap();
    assert_eq!("test0", result);
  }

  async fn test_javascript_query() {
    let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
    conn
      .execute("CREATE TABLE test (v0 TEXT, v1 INTEGER);", ())
      .await
      .unwrap();
    conn
      .execute("INSERT INTO test (v0, v1) VALUES ('0', 0), ('1', 1);", ())
      .await
      .unwrap();

    let handle = RuntimeHandle::new();
    handle.override_connection(conn);

    let module = Module::new(
      "module.ts",
      r#"
        import { query } from "trailbase:main";

        export async function test_query(queryStr: string) : Promise<unknown[][]> {
          return await query(queryStr, []);
        }
      "#,
    );

    let result = handle
      .call_function::<Vec<Vec<serde_json::Value>>>(
        Some(module),
        "test_query",
        vec![serde_json::json!("SELECT * FROM test")],
      )
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
      result
    );
  }

  async fn test_javascript_execute() {
    let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
    conn
      .execute("CREATE TABLE test (v0 TEXT, v1 INTEGER);", ())
      .await
      .unwrap();

    let handle = RuntimeHandle::new();
    handle.override_connection(conn.clone());

    let module = Module::new(
      "module.ts",
      r#"
              import { execute } from "trailbase:main";

              export async function test_execute(queryStr: string) : Promise<number> {
                return await execute(queryStr, []);
              }
            "#,
    );

    let _result = handle
      .call_function::<i64>(
        Some(module),
        "test_execute",
        vec![serde_json::json!("DELETE FROM test")],
      )
      .await
      .unwrap();

    let row = conn
      .query_row("SELECT COUNT(*) FROM test", ())
      .await
      .unwrap()
      .unwrap();
    let count: i64 = row.get(0).unwrap();
    assert_eq!(0, count);
  }
}
