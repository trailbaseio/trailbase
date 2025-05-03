use futures_util::future::LocalBoxFuture;
use log::*;
use rustyscript::{deno_core::PollEventLoopOptions, init_platform, js_value::Promise};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;
use tokio::sync::oneshot;
use tokio::time::Duration;
use tracing_subscriber::prelude::*;
use trailbase_sqlite::rows::{JsonError, row_to_json_array};

use crate::JsRuntimeAssets;
use crate::util::cow_to_string;

pub use rustyscript::{Error, Module, ModuleHandle, Runtime};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Serialize)]
pub struct JsUser {
  // Base64 encoded user id.
  pub id: String,
  pub email: String,
  pub csrf: String,
}

pub type CallbackType =
  dyn FnOnce(Option<&ModuleHandle>, &mut Runtime) -> Option<Box<dyn Completer>> + Send;

pub enum Message {
  Run(Option<Module>, Box<CallbackType>),
}

pub struct State {
  private_sender: kanal::AsyncSender<Message>,
}

impl State {
  pub async fn load_module(&self, module: Module) -> Result<(), AnyError> {
    let (sender, receiver) = oneshot::channel::<Result<(), AnyError>>();

    self
      .private_sender
      .send(Message::Run(
        Some(module),
        Box::new(|module_handle, _runtime| {
          let _ = match module_handle {
            Some(_) => sender.send(Ok(())),
            None => sender.send(Err("Failed to load module".into())),
          };
          return None;
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

struct RuntimeState {
  n_threads: usize,

  // Thread handle
  handle: Option<std::thread::JoinHandle<()>>,

  // Shared sender.
  shared_sender: kanal::AsyncSender<Message>,

  // Isolate state.
  state: Vec<State>,
}

impl Drop for RuntimeState {
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
  fn resolve(self: Box<Self>, runtime: &mut Runtime) -> LocalBoxFuture<'_, ()>;
}

pub struct CompleterImpl<T: serde::de::DeserializeOwned + Send + 'static> {
  /// Promise eventually resolved by the JS engine.
  pub promise: Promise<T>,
  /// Back channel to eventually resolve with the value from the promise above.
  pub sender: oneshot::Sender<Result<T, Error>>,
}

impl<T: serde::de::DeserializeOwned + Send + 'static> Completer for CompleterImpl<T> {
  fn is_ready(&self, runtime: &mut Runtime) -> bool {
    if self.sender.is_closed() {
      return true;
    }
    return !self.promise.is_pending(runtime);
  }

  fn resolve(self: Box<Self>, runtime: &mut Runtime) -> LocalBoxFuture<'_, ()> {
    let sender = self.sender;
    if sender.is_closed() {
      return Box::pin(async {});
    }

    let promise = self.promise;
    Box::pin(async {
      let _ = sender.send(promise.into_future(runtime).await);
    })
  }
}

impl RuntimeState {
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
        let (private_sender, private_receiver) = kanal::unbounded_async::<Message>();

        return (State { private_sender }, private_receiver);
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

    return RuntimeState {
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

    return Ok(runtime);
  }
}

pub fn build_call_sync_js_function_message<T>(
  module: Option<Module>,
  function_name: &'static str,
  args: impl serde::ser::Serialize + Send + 'static,
  response: oneshot::Sender<Result<T, Error>>,
) -> Message
where
  T: serde::de::DeserializeOwned + Send + 'static,
{
  return Message::Run(
    module,
    Box::new(move |module_handle, runtime: &mut Runtime| {
      let _ =
        response.send(runtime.call_function_immediate::<T>(module_handle, function_name, &args));
      return None;
    }),
  );
}

pub fn build_call_async_js_function_message<T>(
  module: Option<Module>,
  function_name: &'static str,
  args: impl serde::ser::Serialize + Send + 'static,
  response: oneshot::Sender<Result<T, Error>>,
) -> Message
where
  T: serde::de::DeserializeOwned + Send + 'static,
{
  return Message::Run(
    module,
    Box::new(move |module_handle, runtime: &mut Runtime| {
      // NOTE: We cannot use `call_function_async` here because it would require `handle_message`
      // to be async and await it, which would prevent new messages from being received until the
      // async function completes. This would lead to a deadlock in case of recursive calls to
      // the same isolate.
      // NOTE: We also cannot push the awaiting to a `LocalSet` local tokio runtime, since it
      // uses `rt.block_on` internally, which makes tokio panic.
      //
      // We haven't found a better way than keeping track of pending futures and resolving them
      // ourselves.
      //
      // Similarly, we await module loading on the event loop hoping that the module load doesn't
      // trigger recursive requests, which would block up the event loop.
      // To get rid off all async calls that require the event-loop to progress, we could build
      // up a module registry before starting the event loop and then refer to modules only by
      // handle afterwards :shrug:.
      let promise_or =
        runtime.call_function_immediate::<Promise<T>>(module_handle, function_name, &args);

      return match promise_or {
        Ok(promise) => Some(Box::new(CompleterImpl::<T> {
          promise,
          sender: response,
        })),
        Err(err) => {
          let _ = response.send(Err(err));
          None
        }
      };
    }),
  );
}

fn drain_filter<T>(v: &mut Vec<T>, mut f: impl FnMut(&T) -> bool) -> Vec<T> {
  let indexes: Vec<usize> = v
    .iter()
    .enumerate()
    .filter_map(|(idx, value)| if f(value) { Some(idx) } else { None })
    .collect();

  return indexes
    .into_iter()
    .rev()
    .map(|index| v.swap_remove(index))
    .collect();
}

/// The main event-loop running for every isolate/worker.
fn event_loop(
  runtime: &mut Runtime,
  private_recv: kanal::AsyncReceiver<Message>,
  shared_recv: kanal::AsyncReceiver<Message>,
) {
  const MODULE_LOAD_TIMEOUT: Duration = Duration::from_millis(1000);
  const DURATION: Option<Duration> = Some(Duration::from_millis(25));
  const OPTS: PollEventLoopOptions = PollEventLoopOptions {
    wait_for_inspector: false,
    pump_v8_message_loop: true,
  };

  runtime.tokio_runtime().block_on(async {
    let mut completers: Vec<Box<dyn Completer>> = vec![];

    loop {
      // In the future, once stabilized, we should use `Vec::drain_filter`.
      for completer in drain_filter(&mut completers, |completer| completer.is_ready(runtime)) {
        completer.resolve(runtime).await;
      }

      let listen_for_messages = async || {
        return tokio::select! {
          msg = private_recv.recv() => msg,
          msg = shared_recv.recv() => msg,
        }.expect("channel closed");
      };

      // Either pump or wait for a new private or shared message.
      tokio::select! {
        // Keep pumping while there are still futures (HTTP requests) that need completing.
        result = runtime.await_event_loop(OPTS, DURATION), if !completers.is_empty() => {
          if let Err(err) = result{
            error!("JS event loop: {err}");
          }
        },
        msg = listen_for_messages() => {
          let completer = match msg {
            Message::Run(module, f) => {
              if let Some(module) = module {
                // Prevent module loads from blocking up the event-loop for ever. This could happen if a
                // top-level call triggers a recursive call to the isolate, while event loop is blocked up
                // awaiting this very `load_module_async` call.
                let Ok(Ok(module_handle)) =
                tokio::time::timeout(MODULE_LOAD_TIMEOUT, runtime.load_module_async(&module)).await else {
                  continue;
                };

                f(Some(&module_handle), runtime)
              } else {
                f(None, runtime)
              }
            }
          };

          if let Some(completer) = completer {
            completers.push(completer);
          }
        },
      }
    }
  });
}

// NOTE: Repeated runtime initialization, e.g. in a multi-threaded context, leads to segfaults.
// rustyscript::init_platform is supposed to help with this but we haven't found a way to
// make it work. Thus, we're making the V8 VM a singleton (like Dart's).
fn get_runtime(n_threads: Option<usize>) -> &'static RuntimeState {
  static SINGLETON: OnceLock<RuntimeState> = OnceLock::new();
  return SINGLETON.get_or_init(move || RuntimeState::new_with_threads(n_threads));
}

#[derive(Clone)]
pub struct RuntimeHandle {
  runtime: &'static RuntimeState,
}

impl RuntimeHandle {
  #[allow(clippy::new_without_default)]
  pub fn singleton() -> Self {
    return Self {
      runtime: get_runtime(None),
    };
  }

  pub fn singleton_or_init_with_threads(n_threads: usize) -> Self {
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
}

pub fn register_database_functions(handle: &RuntimeHandle, conn: trailbase_sqlite::Connection) {
  fn register(runtime: &mut Runtime, conn: trailbase_sqlite::Connection) -> Result<(), Error> {
    let conn_clone = conn.clone();
    runtime.register_async_function("query", move |args: Vec<serde_json::Value>| {
      let conn = conn_clone.clone();
      Box::pin(async move {
        let query: String = get_arg(&args, 0)?;
        let params = json_values_to_params(get_arg(&args, 1)?)?;

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

    let conn_clone = conn.clone();
    runtime.register_async_function("execute", move |args: Vec<serde_json::Value>| {
      let conn = conn_clone.clone();
      Box::pin(async move {
        let query: String = get_arg(&args, 0)?;
        let params = json_values_to_params(get_arg(&args, 1)?)?;

        let rows_affected = conn
          .execute(query, params)
          .await
          .map_err(|err| rustyscript::Error::Runtime(err.to_string()))?;

        return Ok(serde_json::Value::Number(rows_affected.into()));
      })
    })?;

    return Ok(());
  }

  let states = &handle.runtime.state;
  let (sender, receiver) = kanal::bounded(states.len());

  for state in states {
    let conn = conn.clone();
    let sender = sender.clone();

    state
      .private_sender
      .as_sync()
      .send(Message::Run(
        None,
        Box::new(move |_, runtime: &mut Runtime| {
          register(runtime, conn).expect("startup");
          sender.send(()).expect("startup");
          return None;
        }),
      ))
      .expect("startup");
  }

  for _ in 0..states.len() {
    receiver.recv().expect("startup");
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
    // Run on a single thread to make sure that any potential blocking is maximally bad.
    let handle = RuntimeHandle::singleton_or_init_with_threads(1);

    // NOTE: needs to run serially since registration of SQLite connection with singleton v8
    // runtime is racy.
    test_runtime_apply(&handle).await;
    test_runtime_javascript(&handle).await;
    test_runtime_javascript_blocking(&handle).await;
    test_javascript_query(&handle).await;
    test_javascript_execute(&handle).await;
  }

  async fn test_runtime_apply(handle: &RuntimeHandle) {
    let (sender, receiver) = tokio::sync::oneshot::channel::<i64>();

    handle
      .runtime
      .shared_sender
      .send(Message::Run(
        None,
        Box::new(|_m, _rt| {
          sender.send(5).unwrap();
          return None;
        }),
      ))
      .await
      .unwrap();

    assert_eq!(5, receiver.await.unwrap());
  }

  async fn test_runtime_javascript(handle: &RuntimeHandle) {
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
        sender,
      ))
      .await
      .unwrap();

    assert_eq!("test0", receiver.await.unwrap().unwrap());
  }

  async fn test_runtime_javascript_blocking(handle: &RuntimeHandle) {
    tracing_subscriber::Registry::default()
      .with(tracing_subscriber::filter::LevelFilter::WARN)
      .set_default();

    let (ext_sender, ext_receiver) = kanal::bounded_async::<i64>(10);
    {
      // Register custom functions.
      let states = &handle.runtime.state;
      let (sender, receiver) = kanal::bounded(states.len());

      for state in states {
        let sender = sender.clone();
        let ext_receiver = ext_receiver.clone();

        state
          .private_sender
          .as_sync()
          .send(Message::Run(
            None,
            Box::new(move |_, runtime| {
              runtime
                .register_async_function("blocked", move |_args: Vec<serde_json::Value>| {
                  let ext_receiver = ext_receiver.clone();
                  return Box::pin(async move {
                    let _ = ext_receiver.recv().await.unwrap();
                    return Ok(serde_json::Value::Null);
                  });
                })
                .expect("register");

              sender.send(()).unwrap();

              return None;
            }),
          ))
          .expect("startup");
      }

      for _ in 0..states.len() {
        receiver.recv().expect("startup");
      }
    }

    let module = Module::new(
      "module.js",
      r#"
        export function test_fun() {
          return "test0";
        }

        export async function blocked_fun() {
          await rustyscript.async_functions.blocked();
          return "blocked";
        }
      "#,
    );

    let (blocked_sender, blocked_receiver) = oneshot::channel::<Result<String, Error>>();
    handle
      .runtime
      .shared_sender
      .send(build_call_async_js_function_message::<String>(
        Some(module.clone()),
        "blocked_fun",
        Vec::<serde_json::Value>::new(),
        blocked_sender,
      ))
      .await
      .unwrap();

    let (sender, receiver) = oneshot::channel::<Result<String, Error>>();
    handle
      .runtime
      .shared_sender
      .send(build_call_sync_js_function_message::<String>(
        Some(module.clone()),
        "test_fun",
        Vec::<serde_json::Value>::new(),
        sender,
      ))
      .await
      .unwrap();

    assert_eq!("test0", receiver.await.unwrap().unwrap());

    ext_sender.send(1).await.unwrap();
    assert_eq!("blocked", blocked_receiver.await.unwrap().unwrap());
  }

  async fn test_javascript_query(handle: &RuntimeHandle) {
    let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
    conn
      .execute("CREATE TABLE test (v0 TEXT, v1 INTEGER);", ())
      .await
      .unwrap();
    conn
      .execute("INSERT INTO test (v0, v1) VALUES ('0', 0), ('1', 1);", ())
      .await
      .unwrap();

    register_database_functions(&handle, conn);

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
        Some(module),
        "test_query",
        vec![serde_json::json!("SELECT * FROM test")],
        sender,
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

  async fn test_javascript_execute(handle: &RuntimeHandle) {
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

    register_database_functions(&handle, conn.clone());

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
        Some(module),
        "test_execute",
        vec![serde_json::json!("DELETE FROM test")],
        sender,
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
