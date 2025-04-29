use axum::body::Body;
use axum::http::{StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use futures_util::future::LocalBoxFuture;
use log::*;
use parking_lot::Mutex;
use rustyscript::{deno_core::PollEventLoopOptions, init_platform, js_value::Promise};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::oneshot;
use tracing_subscriber::prelude::*;
use trailbase_sqlite::rows::{JsonError, row_to_json_array};

use crate::JsRuntimeAssets;
use crate::util::cow_to_string;

pub use rustyscript::{Error, Module, ModuleHandle, Runtime};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Deserialize, Default, Debug)]
pub struct JsHttpResponse {
  pub headers: Option<Vec<(String, String)>>,
  pub status: Option<u16>,
  pub body: Option<bytes::Bytes>,
}

#[derive(Debug, Error)]
pub enum JsHttpResponseError {
  #[error("Precondition: {0}")]
  Precondition(String),
  #[error("Internal: {0}")]
  Internal(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Serialize)]
pub struct JsUser {
  // Base64 encoded user id.
  pub id: String,
  pub email: String,
  pub csrf: String,
}

pub struct DispatchArgs {
  pub method: String,
  pub route_path: String,
  pub uri: String,
  pub path_params: Vec<(String, String)>,
  pub headers: Vec<(String, String)>,
  pub user: Option<JsUser>,
  pub body: bytes::Bytes,

  pub reply: oneshot::Sender<Result<JsHttpResponse, JsHttpResponseError>>,
}

#[allow(clippy::type_complexity)]
pub enum Message {
  Run(
    Option<Module>,
    Box<dyn (FnOnce(Option<&ModuleHandle>, &mut Runtime, &mut Vec<Box<dyn Completer>>)) + Send>,
  ),
}

pub struct State {
  private_sender: kanal::AsyncSender<Message>,
  connection: Mutex<Option<trailbase_sqlite::Connection>>,
}

impl State {
  pub async fn load_module(&self, module: Module) -> Result<(), AnyError> {
    let (sender, receiver) = oneshot::channel::<Result<(), AnyError>>();

    self
      .private_sender
      .send(Message::Run(
        Some(module),
        Box::new(|module_handle, _runtime, _completers| {
          let _ = match module_handle {
            Some(_) => sender.send(Ok(())),
            None => sender.send(Err("Failed to load module".into())),
          };
        }),
      ))
      .await?;

    let _ = receiver.await.map_err(|err| {
      error!("Failed to await module loading: {err}");
      return err;
    })?;

    return Ok(());
  }

  pub async fn send_privately(&self, msg: Message) -> Result<(), kanal::SendError> {
    return self.private_sender.send(msg).await;
  }
}

struct RuntimeSingleton {
  n_threads: usize,

  // Thread handle
  handle: Option<std::thread::JoinHandle<()>>,

  // Shared sender.
  shared_sender: kanal::AsyncSender<Message>,

  // Isolate state.
  state: Vec<State>,
}

impl Drop for RuntimeSingleton {
  fn drop(&mut self) {
    if let Some(handle) = self.handle.take() {
      self.state.clear();
      if let Err(err) = handle.join() {
        error!("Failed to join main rt thread: {err:?}");
      }
    }
  }
}

pub trait Completer {
  fn is_ready(&self, runtime: &mut Runtime) -> bool;
  fn resolve<'a>(self: Box<Self>, runtime: &'a mut Runtime) -> LocalBoxFuture<'a, ()>;
}

pub struct CompleterImpl<T: serde::de::DeserializeOwned + Send + 'static> {
  /// Identifier for book-keeping.
  #[allow(unused)]
  pub name: String,
  /// Promise eventually resolved by the JS engine.
  pub promise: Promise<T>,
  /// Back channel to eventually resolve with the value from the promise above.
  pub resolver: Box<dyn FnOnce(Result<T, Error>) + Send>,
}

impl<T: serde::de::DeserializeOwned + Send + 'static> Completer for CompleterImpl<T> {
  fn is_ready(&self, runtime: &mut Runtime) -> bool {
    return !self.promise.is_pending(runtime);
  }

  fn resolve<'a>(self: Box<Self>, runtime: &'a mut Runtime) -> LocalBoxFuture<'a, ()> {
    let resolver = self.resolver;
    let promise = self.promise;
    Box::pin(async {
      resolver(promise.into_future(runtime).await);
    })
  }
}

impl RuntimeSingleton {
  /// Bring up `threads` worker/isolate threads with basic setup.
  ///
  /// NOTE: functions to install routes and jobs are registered later, we need an AppState first.
  fn new_with_threads(threads: Option<usize>) -> Self {
    let n_threads = match threads {
      Some(n) => n,
      None => std::thread::available_parallelism().map_or_else(
        |err| {
          error!("Failed to get number of threads: {err}");
          return 1;
        },
        |x| x.get(),
      ),
    };

    info!("Starting v8 JavaScript runtime with {n_threads} workers.");

    let (shared_sender, shared_receiver) = kanal::unbounded_async::<Message>();

    let (state, receivers): (Vec<State>, Vec<kanal::AsyncReceiver<Message>>) = (0..n_threads)
      .map(|_index| {
        let (sender, receiver) = kanal::unbounded_async::<Message>();

        return (
          State {
            private_sender: sender,
            connection: Mutex::new(None),
          },
          receiver,
        );
      })
      .unzip();

    let handle = if n_threads > 0 {
      Some(std::thread::spawn(move || {
        // swc_ecma_codegen is very spammy (or at least used to be):
        //   https://github.com/swc-project/swc/pull/9604
        tracing_subscriber::Registry::default()
          .with(tracing_subscriber::filter::Targets::new().with_target(
            "tracing::span",
            tracing_subscriber::filter::LevelFilter::WARN,
          ))
          .set_default();

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
                  .thread_name(format!("v8-runtime-{index}"))
                  .build()
                  .expect("startup"),
              );

              let mut js_runtime = match Self::init_runtime(index, tokio_runtime.clone()) {
                Ok(js_runtime) => js_runtime,
                Err(err) => {
                  panic!("Failed to init v8 runtime on thread {index}: {err}");
                }
              };

              event_loop(&mut js_runtime, receiver, shared_receiver);
            });
          })
          .collect();

        for (idx, thread) in threads.into_iter().enumerate() {
          if let Err(err) = thread.join() {
            error!("Failed to join worker: {idx}: {err:?}");
          }
        }
      }))
    } else {
      None
    };

    return RuntimeSingleton {
      n_threads,
      shared_sender,
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
        import_provider: Some(Box::new(crate::import_provider::ImportProvider)),
        schema_whlist: HashSet::from(["trailbase".to_string()]),
        ..Default::default()
      },
      tokio_runtime,
    )?;

    runtime
      .register_function("isolate_id", move |_args: &[serde_json::Value]| {
        return Ok(serde_json::json!(index));
      })
      .expect("Failed to register 'isolate_id' function");

    runtime.register_async_function("query", move |args: Vec<serde_json::Value>| {
      Box::pin(async move {
        let query: String = get_arg(&args, 0)?;
        let params = json_values_to_params(get_arg(&args, 1)?)?;

        let Some(conn) = get_runtime(None).state[index].connection.lock().clone() else {
          return Err(rustyscript::Error::Runtime(
            "missing db connection".to_string(),
          ));
        };

        let rows = conn
          .write_query_rows(query, params)
          .await
          .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

        let values = rows
          .iter()
          .map(|row| -> Result<serde_json::Value, JsonError> {
            return Ok(serde_json::Value::Array(row_to_json_array(row)?));
          })
          .collect::<Result<Vec<_>, _>>()
          .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

        return Ok(serde_json::Value::Array(values));
      })
    })?;

    runtime.register_async_function("execute", move |args: Vec<serde_json::Value>| {
      Box::pin(async move {
        let query: String = get_arg(&args, 0)?;
        let params = json_values_to_params(get_arg(&args, 1)?)?;

        let Some(conn) = get_runtime(None).state[index].connection.lock().clone() else {
          return Err(rustyscript::Error::Runtime(
            "missing db connection".to_string(),
          ));
        };

        let rows_affected = conn
          .execute(query, params)
          .await
          .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

        return Ok(serde_json::Value::Number(rows_affected.into()));
      })
    })?;

    return Ok(runtime);
  }
}

pub fn build_call_sync_js_function_message<T>(
  module: Option<Module>,
  function_name: &'static str,
  args: impl serde::ser::Serialize + Send + 'static,
  resolver: impl FnOnce(Result<T, Error>) + Send + 'static,
) -> Message
where
  T: serde::de::DeserializeOwned + Send,
{
  return Message::Run(
    module,
    Box::new(
      move |module_handle, runtime: &mut Runtime, _completers: &mut Vec<Box<dyn Completer>>| {
        resolver(runtime.call_function_immediate::<T>(module_handle, function_name, &args));
      },
    ),
  );
}

pub fn build_call_async_js_function_message<T>(
  id: String,
  module: Option<Module>,
  function_name: &'static str,
  args: impl serde::ser::Serialize + Send + 'static,
  resolver: impl FnOnce(Result<T, Error>) + Send + 'static,
) -> Message
where
  T: serde::de::DeserializeOwned + Send + 'static,
{
  return Message::Run(
    module,
    Box::new(
      move |module_handle, runtime: &mut Runtime, completers: &mut Vec<Box<dyn Completer>>| {
        let promise_or =
          runtime.call_function_immediate::<Promise<T>>(module_handle, function_name, &args);

        match promise_or {
          Ok(promise) => {
            completers.push(Box::new(CompleterImpl::<T> {
              name: id,
              promise,
              resolver: Box::new(resolver),
            }));
          }
          Err(err) => resolver(Err(err)),
        };
      },
    ),
  );
}

pub fn build_http_dispatch_message(args: DispatchArgs) -> Message {
  return build_call_async_js_function_message(
    args.uri.clone(),
    None,
    "__dispatch",
    serde_json::json!([
      args.method,
      args.route_path,
      args.uri,
      args.path_params,
      args.headers,
      args.user,
      args.body
    ]),
    move |value_or: Result<JsHttpResponse, Error>| {
      if args
        .reply
        .send(value_or.map_err(|err| JsHttpResponseError::Internal(err.into())))
        .is_err()
      {
        debug!("Failed to send reply. Channel closed");
      }
    },
  );
}

#[inline]
async fn handle_message(
  runtime: &mut Runtime,
  msg: Message,
  completers: &mut Vec<Box<dyn Completer>>,
) -> Result<(), AnyError> {
  match msg {
    Message::Run(module, f) => {
      if let Some(module) = module {
        let module_handle = runtime.load_module_async(&module).await?;
        f(Some(&module_handle), runtime, completers);
      } else {
        f(None, runtime, completers);
      }
    }
  }

  return Ok(());
}

/// The main event-loop running for every isolate/worker.
fn event_loop(
  runtime: &mut Runtime,
  private_recv: kanal::AsyncReceiver<Message>,
  shared_recv: kanal::AsyncReceiver<Message>,
) {
  const DURATION: Option<Duration> = Some(Duration::from_millis(25));
  const OPTS: PollEventLoopOptions = PollEventLoopOptions {
    wait_for_inspector: false,
    pump_v8_message_loop: true,
  };

  runtime.tokio_runtime().block_on(async {
    let mut completers: Vec<Box<dyn Completer>> = vec![];

    loop {
      let completed_indexes = completers
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

      for index in completed_indexes.into_iter().rev() {
        completers.swap_remove(index).resolve(runtime).await;
      }

      // Either pump or wait for a new private or shared message.
      tokio::select! {
        // Keep pumping while there are still futures (HTTP requests) that need completing.
        result = runtime.await_event_loop(OPTS, DURATION), if !completers.is_empty() => {
          if let Err(err) = result{
            error!("JS event loop: {err}");
          }
        },
        msg = private_recv.recv() => {
          let Ok(msg) = msg else {
            panic!("private channel closed");
          };
          if let Err(err) = handle_message(runtime, msg, &mut completers).await {
            error!("Handle private message: {err}");
          }
        },
        msg = shared_recv.recv() => {
          let Ok(msg) = msg else {
            panic!("private channel closed");
          };
          if let Err(err) = handle_message(runtime, msg, &mut completers).await {
            error!("Handle shared message: {err}");
          }
        },
      }
    }
  });
}

// NOTE: Repeated runtime initialization, e.g. in a multi-threaded context, leads to segfaults.
// rustyscript::init_platform is supposed to help with this but we haven't found a way to
// make it work. Thus, we're making the V8 VM a singleton (like Dart's).
fn get_runtime(n_threads: Option<usize>) -> &'static RuntimeSingleton {
  static RUNTIME: OnceLock<RuntimeSingleton> = OnceLock::new();
  return RUNTIME.get_or_init(move || RuntimeSingleton::new_with_threads(n_threads));
}

#[derive(Clone)]
pub struct RuntimeHandle {
  runtime: &'static RuntimeSingleton,
}

impl RuntimeHandle {
  #[allow(clippy::new_without_default)]
  pub fn new() -> Self {
    return Self {
      runtime: get_runtime(None),
    };
  }

  pub fn new_with_threads(n_threads: usize) -> Self {
    return Self {
      runtime: get_runtime(Some(n_threads)),
    };
  }

  pub fn num_threads(&self) -> usize {
    return self.runtime.n_threads;
  }

  pub fn state(&self) -> &'static Vec<State> {
    return &self.runtime.state;
  }

  pub async fn send_to_any_isolate(&self, msg: Message) -> Result<(), kanal::SendError> {
    return self.runtime.shared_sender.send(msg).await;
  }

  pub fn set_connection(&self, conn: trailbase_sqlite::Connection, r#override: bool) {
    for s in &self.runtime.state {
      let mut lock = s.connection.lock();
      if lock.is_some() {
        if !r#override {
          panic!("connection already set");
        }

        debug!("connection already set");
      } else {
        lock.replace(conn.clone());
      }
    }
  }
}

fn json_value_to_param(
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

fn json_values_to_params(
  values: Vec<serde_json::Value>,
) -> Result<Vec<trailbase_sqlite::Value>, rustyscript::Error> {
  return values.into_iter().map(json_value_to_param).collect();
}

impl IntoResponse for JsHttpResponseError {
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

pub fn get_arg<T>(args: &[serde_json::Value], i: usize) -> Result<T, rustyscript::Error>
where
  T: serde::de::DeserializeOwned,
{
  use rustyscript::Error;
  let arg = args
    .get(i)
    .ok_or_else(|| Error::Runtime(format!("Range err {i} > {}", args.len())))?;
  return serde_json::from_value::<T>(arg.clone()).map_err(|err| Error::Runtime(err.to_string()));
}

pub async fn write_js_runtime_files(data_dir: impl AsRef<Path>) {
  let path = data_dir.as_ref();
  if let Err(err) = tokio::fs::write(
    path.join("trailbase.js"),
    cow_to_string(
      JsRuntimeAssets::get("index.js")
        .expect("Failed to read rt/index.js")
        .data,
    )
    .as_str(),
  )
  .await
  {
    warn!("Failed to write 'trailbase.js': {err}");
  }

  if let Err(err) = tokio::fs::write(
    path.join("trailbase.d.ts"),
    cow_to_string(
      JsRuntimeAssets::get("index.d.ts")
        .expect("Failed to read rt/index.d.ts")
        .data,
    )
    .as_str(),
  )
  .await
  {
    warn!("Failed to write 'trailbase.d.ts': {err}");
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
      .shared_sender
      .send(Message::Run(
        None,
        Box::new(|_m, _rt, _c| {
          sender.send(5).unwrap();
        }),
      ))
      .await
      .unwrap();

    assert_eq!(5, receiver.await.unwrap());
  }

  async fn test_runtime_javascript() {
    let handle = RuntimeHandle::new();

    tracing_subscriber::Registry::default()
      .with(tracing_subscriber::filter::LevelFilter::WARN)
      .set_default();
    let module = Module::new(
      "module.js",
      r#"
        export function test_fun() {
          return "test0";
        }
      "#,
    );

    let (sender, receiver) = oneshot::channel::<Result<String, Error>>();
    handle
      .runtime
      .shared_sender
      .send(build_call_sync_js_function_message::<String>(
        Some(module),
        "test_fun",
        Vec::<serde_json::Value>::new(),
        move |value_or| {
          sender.send(value_or).unwrap();
        },
      ))
      .await
      .unwrap();

    assert_eq!("test0", receiver.await.unwrap().unwrap());
  }

  fn override_connection(handle: &RuntimeHandle, conn: trailbase_sqlite::Connection) {
    for s in &handle.runtime.state {
      let mut lock = s.connection.lock();
      if lock.is_some() {
        debug!("connection already set");
      }
      lock.replace(conn.clone());
    }
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
    override_connection(&handle, conn);

    tracing_subscriber::Registry::default()
      .with(tracing_subscriber::filter::LevelFilter::WARN)
      .set_default();
    let module = Module::new(
      "module.ts",
      r#"
        import { query } from "trailbase:main";

        export async function test_query(queryStr: string) : Promise<unknown[][]> {
          return await query(queryStr, []);
        }
      "#,
    );

    let (sender, receiver) = oneshot::channel();
    handle
      .send_to_any_isolate(build_call_async_js_function_message::<
        Vec<Vec<serde_json::Value>>,
      >(
        "<SOME ID>".to_string(),
        Some(module),
        "test_query",
        vec![serde_json::json!("SELECT * FROM test")],
        move |value_or| {
          sender.send(value_or).unwrap();
        },
      ))
      .await
      .unwrap();

    let result = receiver.await.unwrap().unwrap();

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
      .execute_batch(
        r#"
          CREATE TABLE test (v0 TEXT, v1 INTEGER);
          INSERT INTO test (v0, v1) VALUES ('foo', 5), ('bar', 3);
        "#,
      )
      .await
      .unwrap();

    let handle = RuntimeHandle::new();
    override_connection(&handle, conn.clone());

    tracing_subscriber::Registry::default()
      .with(tracing_subscriber::filter::LevelFilter::WARN)
      .set_default();
    let module = Module::new(
      "module.ts",
      r#"
        import { execute } from "trailbase:main";

        export async function test_execute(queryStr: string) : Promise<number> {
          return await execute(queryStr, []);
        }
      "#,
    );

    let (sender, receiver) = oneshot::channel();
    handle
      .send_to_any_isolate(build_call_async_js_function_message::<i64>(
        "<SOME ID>".to_string(),
        Some(module),
        "test_execute",
        vec![serde_json::json!("DELETE FROM test")],
        move |value_or| {
          sender.send(value_or).unwrap();
        },
      ))
      .await
      .unwrap();

    let result = receiver.await.unwrap().unwrap();
    assert_eq!(2, result);

    let count: i64 = conn
      .read_query_row_f("SELECT COUNT(*) FROM test", (), |row| row.get(0))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(0, count);
  }
}
