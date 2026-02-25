#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod functions;
mod host;
mod sqlite;

use bytes::Bytes;
use core::future::Future;
use http_body_util::combinators::UnsyncBoxBody;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;
use tokio::sync::Mutex;
use tokio::task::JoinError;
use trailbase_wasi_keyvalue::WasiKeyValueCtx;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{AsContextMut, Config, Engine, Result, Store};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};
use wasmtime_wasi_http::bindings::http::types::ErrorCode;
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

use host::exports::trailbase::component::init_endpoint::Arguments;

pub use host::exports::trailbase::component::init_endpoint::HttpMethodType;
pub use host::{SharedState, State};
pub use trailbase_wasi_keyvalue::Store as KvStore;

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Wasmtime: {0}")]
  Wasmtime(#[from] wasmtime::Error),
  #[error("Channel closed")]
  ChannelClosed,
  #[error("Http Error: {0}")]
  HttpErrorCode(ErrorCode),
  #[error("Encoding")]
  Encoding,
  #[error("Other: {0}")]
  Other(String),
}

#[derive(Clone, Default, Debug)]
pub struct RuntimeOptions {
  /// Optional file-system sandbox root for r/o file access.
  pub fs_root_path: Option<PathBuf>,

  /// Whether to use the non-optimizing baseline compiler.
  pub use_winch: bool,

  /// Which tokio runtime handle to execute on.
  pub tokio_runtime: Option<tokio::runtime::Handle>,
}

pub trait StoreBuilder<S> {
  fn new_store(&self, engine: &Engine) -> Result<Store<S>, Error>;
}

// NOTE: A better name may be Component.
struct RuntimeInternal<T: StoreBuilder<State>> {
  engine: Engine,
  linker: Linker<State>,

  component: Component,
  /// Path to original .wasm component file.
  component_path: PathBuf,

  store_builder: T,

  rt_handle: tokio::runtime::Handle,
  local_in_flight: AtomicUsize,
}

#[derive(Clone)]
pub struct RuntimeT<T: StoreBuilder<State>> {
  state: Arc<RuntimeInternal<T>>,
}

pub type Runtime = RuntimeT<Arc<SharedState>>;

impl<T: StoreBuilder<State>> RuntimeT<T> {
  pub fn init(
    wasm_source_file: PathBuf,
    store_builder: T,
    opts: RuntimeOptions,
  ) -> Result<Self, Error> {
    let engine = {
      let cache = wasmtime::Cache::new(wasmtime::CacheConfig::default())?;
      let config = build_config(Some(cache), opts.use_winch);

      Engine::new(&config)?
    };

    // Load the component - a very expensive operation generating code. Compilation happens in
    // parallel and will saturate the entire machine.
    let component = {
      log::info!("Compiling: {wasm_source_file:?}. May take some time...");

      let start = SystemTime::now();
      let component = wasmtime::CodeBuilder::new(&engine)
        .wasm_binary_or_text_file(&wasm_source_file)?
        .compile_component()?;

      // NOTE: According to docs, this should not do anything (it seems like a reasonable thing to
      // call explicitly).
      component.initialize_copy_on_write_image()?;

      log::info!(
        "Loaded component {wasm_source_file:?} in: {elapsed:?}.",
        elapsed = SystemTime::now().duration_since(start).unwrap_or_default()
      );

      component
    };

    let linker = {
      let mut linker = Linker::<State>::new(&engine);

      // Adds all the default WASI implementations: clocks, random, fs, ...
      wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;

      // Adds default HTTP interfaces - incoming and outgoing.
      wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)?;

      // Add default KV interfaces.
      trailbase_wasi_keyvalue::add_to_linker(&mut linker, |cx| {
        trailbase_wasi_keyvalue::WasiKeyValue::new(&cx.kv, &mut cx.resource_table)
      })?;

      // Host interfaces.
      host::trailbase::database::sqlite::add_to_linker::<_, State>(&mut linker, |s| s)?;

      linker
    };

    let rt_handle = opts.tokio_runtime.unwrap_or_else(|| {
      log::debug!("Re-using Tokio runtime from context");
      tokio::runtime::Handle::current()
    });

    let state = Arc::new(RuntimeInternal {
      engine,
      linker,
      component,
      component_path: wasm_source_file,
      store_builder,
      rt_handle,
      local_in_flight: AtomicUsize::new(0),
    });

    return Ok(Self { state });
  }

  pub fn component_path(&self) -> &PathBuf {
    return &self.state.component_path;
  }

  async fn new_bindings(&self) -> Result<(Store<State>, crate::host::Interfaces), Error> {
    let mut store = self.state.store_builder.new_store(&self.state.engine)?;

    let bindings = crate::host::Interfaces::instantiate_async(
      &mut store,
      &self.state.component,
      &self.state.linker,
    )
    .await
    .map_err(|err| {
      log::error!(
        "Failed to instantiate WIT component {path:?}: '{err}'.\n{ABI_MISMATCH_WARNING}",
        path = self.state.component_path
      );
      return err;
    })?;

    return Ok((store, bindings));
  }
}

pub struct InitArgs {
  pub version: Option<String>,
}

pub struct InitResult {
  /// Registered http handlers (method, path)[].
  pub http_handlers: Vec<(HttpMethodType, String)>,

  /// Registered jobs (name, spec)[].
  pub job_handlers: Vec<(String, String)>,
}

impl StoreBuilder<State> for Arc<SharedState> {
  fn new_store(&self, engine: &Engine) -> Result<Store<State>, Error> {
    let mut wasi_ctx = WasiCtxBuilder::new();
    wasi_ctx.inherit_stdio();
    wasi_ctx.stdin(wasmtime_wasi::p2::pipe::ClosedInputStream);
    // wasi_ctx.stdout(wasmtime_wasi::p2::Stdout);
    // wasi_ctx.stderr(wasmtime_wasi::p2::Stderr);

    wasi_ctx.args(&[""]);
    wasi_ctx.allow_tcp(false);
    wasi_ctx.allow_udp(false);
    wasi_ctx.allow_ip_name_lookup(true);

    if let Some(ref path) = self.fs_root_path {
      wasi_ctx
        .preopened_dir(path, "/", DirPerms::READ, FilePerms::READ)
        .map_err(|err| Error::Other(err.to_string()))?;
    }

    return Ok(Store::new(
      engine,
      State {
        resource_table: ResourceTable::new(),
        wasi_ctx: wasi_ctx.build(),
        http: WasiHttpCtx::new(),
        kv: WasiKeyValueCtx::new(self.kv_store.clone()),
        shared: self.clone(),
        tx: Arc::new(parking_lot::Mutex::new(None)),
      },
    ));
  }
}

struct HttpStoreInternal {
  store: Mutex<Store<State>>,
  bindings: crate::host::Interfaces,
  proxy_bindings: wasmtime_wasi_http::bindings::Proxy,

  runtime_state: Arc<RuntimeInternal<Arc<SharedState>>>,
}

#[derive(Clone)]
pub struct HttpStore {
  state: Arc<HttpStoreInternal>,
}

impl HttpStore {
  pub async fn new(rt: &Runtime) -> Result<Self, Error> {
    let (mut store, bindings) = rt.new_bindings().await?;

    let proxy_bindings = wasmtime_wasi_http::bindings::Proxy::instantiate_async(
      &mut store,
      &rt.state.component,
      &rt.state.linker,
    )
    .await?;

    return Ok(Self {
      state: Arc::new(HttpStoreInternal {
        store: Mutex::new(store),
        bindings,
        proxy_bindings,
        runtime_state: rt.state.clone(),
      }),
    });
  }

  pub async fn initialize(&self, args: InitArgs) -> Result<InitResult, Error> {
    let state = self.state.clone();

    return Self::call(&self.state.runtime_state, async move {
      let api = state.bindings.trailbase_component_init_endpoint();

      let args = Arguments {
        version: args.version,
      };

      let mut store = state.store.lock().await;
      store
        .run_concurrent(async |accessor| -> Result<InitResult, Error> {
          let (http, task_exit) = api.call_init_http_handlers(accessor, args.clone()).await?;
          task_exit.block(accessor).await;

          let (job, task_exit) = api.call_init_job_handlers(accessor, args).await?;
          task_exit.block(accessor).await;

          return Ok(InitResult {
            http_handlers: http.handlers,
            job_handlers: job.handlers,
          });
        })
        .await?
    })
    .await
    .map_err(|join_err| Error::Other(join_err.to_string()))?;
  }

  pub async fn call_incoming_http_handler(
    &self,
    request: hyper::Request<UnsyncBoxBody<Bytes, hyper::Error>>,
  ) -> Result<hyper::Response<wasmtime_wasi_http::body::HyperOutgoingBody>, Error> {
    let state = self.state.clone();

    return Self::call(&self.state.runtime_state, async move {
      let (sender, receiver) = tokio::sync::oneshot::channel::<
        Result<hyper::Response<wasmtime_wasi_http::body::HyperOutgoingBody>, ErrorCode>,
      >();

      // NOTE: wstd streams out responses in chunks of 2kB. Only once everything has been
      // streamed, `call_handle` will complete. This is also when the streaming response
      // body completes.
      //
      // We cannot use `wasmtime_wasi::runtime::spawn` here, which aborts the call when the handle
      // gets dropped, since we're not awaiting the response stream here. We'd either have to
      // consume the entire response here, keep the handle alive or as we currently do use a
      // non-aborting spawn.
      //
      // In the current setup, if the listening side hangs-up the they call may not be aborted.
      // Depends on what the implementation does when the streaming body's receiving end gets
      // out of scope.
      let handle = tokio::spawn(async move {
        let mut lock = state.store.lock().await;

        let req = lock.data_mut().new_incoming_request(
          wasmtime_wasi_http::bindings::http::types::Scheme::Http,
          request,
        )?;

        let out = lock.data_mut().new_response_outparam(sender)?;

        state
          .proxy_bindings
          .wasi_http_incoming_handler()
          .call_handle(lock.as_context_mut(), req, out)
          .await
      });

      match receiver.await {
        Ok(Ok(resp)) => {
          // NOTE: We cannot await the completion `call_handle` here with `handle.await?;`, since
          // we're not consuming the response body, see above.
          Ok(resp)
        }
        Ok(Err(err)) => {
          handle
            .await
            .map_err(|err| Error::Other(err.to_string()))??;
          Err(Error::HttpErrorCode(err))
        }
        Err(_) => {
          log::debug!("channel closed");
          handle
            .await
            .map_err(|err| Error::Other(err.to_string()))??;
          Err(Error::ChannelClosed)
        }
      }
    })
    .await
    .map_err(|join_err| Error::Other(join_err.to_string()))?;
  }

  fn call<F>(
    rt: &Arc<RuntimeInternal<Arc<SharedState>>>,
    f: F,
  ) -> impl Future<Output = Result<F::Output, JoinError>>
  where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
  {
    let state = rt.clone();

    #[cfg(debug_assertions)]
    log::debug!(
      "WASM runtime ({path:?}) waiting for new messages. In flight: {}, {}",
      state.local_in_flight.load(Ordering::Relaxed),
      IN_FLIGHT.load(Ordering::Relaxed),
      path = state.component_path,
    );

    state.local_in_flight.fetch_add(1, Ordering::Relaxed);
    IN_FLIGHT.fetch_add(1, Ordering::Relaxed);

    return rt.rt_handle.spawn(async move {
      let r = f.await;

      IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
      state.local_in_flight.fetch_sub(1, Ordering::Relaxed);

      r
    });
  }
}

pub fn find_wasm_components(components_path: impl AsRef<std::path::Path>) -> Vec<PathBuf> {
  let Ok(dir) = std::fs::read_dir(components_path.as_ref()) else {
    return vec![];
  };

  return dir
    .into_iter()
    .flat_map(|entry| {
      let Ok(entry) = entry else {
        return None;
      };

      let Ok(metadata) = entry.metadata() else {
        return None;
      };

      if !metadata.is_file() {
        return None;
      }

      let path = entry.path();
      if path.extension()? == "wasm" {
        return Some(path);
      }
      return None;
    })
    .collect();
}

fn build_config(cache: Option<wasmtime::Cache>, use_winch: bool) -> Config {
  let mut config = Config::new();

  // Execution settings:
  config.epoch_interruption(false);
  config.memory_reservation(64 * 1024 * 1024 /* bytes */);
  config.wasm_component_model(true);
  // config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);

  // Compilation settings.
  config.cache(cache);

  if use_winch {
    config.strategy(wasmtime::Strategy::Winch);
  } else {
    config.strategy(wasmtime::Strategy::Cranelift);
    config.cranelift_opt_level(wasmtime::OptLevel::Speed);
    config.parallel_compilation(true);
  }

  return config;
}

// fn bytes_to_response(
//   bytes: Vec<u8>,
// ) -> Result<wasmtime_wasi_http::types::HostFutureIncomingResponse, ErrorCode> {
//   let resp = http::Response::builder()
//     .status(200)
//     .body(sqlite::bytes_to_body(Bytes::from_owner(bytes)))
//     .map_err(|err| ErrorCode::InternalError(Some(err.to_string())))?;
//
//   return Ok(
//     wasmtime_wasi_http::types::HostFutureIncomingResponse::ready(Ok(Ok(
//       wasmtime_wasi_http::types::IncomingResponse {
//         resp,
//         worker: None,
//         between_bytes_timeout: std::time::Duration::ZERO,
//       },
//     ))),
//   );
// }
//

const ABI_MISMATCH_WARNING: &str = "\
    This may happen if the server and component are ABI incompatible. Make sure to run compatible \
    versions, i.e. update/rebuild the component to match the server binary or update your server \
    to run more up-to-date components.\n\
    First-party components can be updated easily by running `$ trail components update` or downloaded from: \
    https://github.com/trailbaseio/trailbase/releases.";
//
#[cfg(test)]
mod tests {
  use super::*;

  use http::{Response, StatusCode};
  use http_body_util::{BodyExt, combinators::UnsyncBoxBody};
  use trailbase_wasm_common::{HttpContext, HttpContextKind};

  use crate::host::SharedState;

  const WASM_COMPONENT_PATH: &str = "../../client/testfixture/wasm/wasm_guest_testfixture.wasm";

  fn init_runtime(conn: Option<trailbase_sqlite::Connection>) -> Runtime {
    let shared_state = Arc::new(SharedState {
      conn,
      kv_store: KvStore::new(),
      fs_root_path: None,
    });

    return Runtime::init(
      WASM_COMPONENT_PATH.into(),
      shared_state,
      RuntimeOptions {
        ..Default::default()
      },
    )
    .unwrap();
  }

  async fn init_sqlite_function_runtime(conn: &rusqlite::Connection) -> Runtime {
    let runtime = init_runtime(None);

    let store = functions::SqliteStore::new(&runtime).await.unwrap();

    let functions = store
      .initialize_sqlite_functions(InitArgs { version: None })
      .await
      .unwrap();

    functions::setup_connection(conn, store, &functions).unwrap();

    return runtime;
  }

  #[tokio::test]
  async fn test_init() {
    let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
    let runtime = init_runtime(Some(conn.clone()));

    let store = HttpStore::new(&runtime).await.unwrap();
    store.initialize(InitArgs { version: None }).await.unwrap();

    let response = send_http_request(
      &runtime,
      "http://localhost:4000/transaction",
      "/transaction",
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK, "{response:?}");

    assert_eq!(
      1,
      conn
        .query_row_f("SELECT COUNT(*) FROM tx;", (), |row| row.get::<_, i64>(0))
        .await
        .unwrap()
        .unwrap()
    )
  }

  #[tokio::test]
  async fn test_transaction() {
    let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
    let runtime = Arc::new(init_runtime(Some(conn.clone())));

    let futures: Vec<_> = (0..256)
      .map(|_| {
        let runtime = runtime.clone();
        tokio::spawn(async move {
          send_http_request(
            &runtime,
            "http://localhost:4000/transaction",
            "/transaction",
          )
          .await
        })
      })
      .collect();

    for future in futures {
      future.await.unwrap().unwrap();
    }
  }

  #[tokio::test]
  async fn test_custom_sqlite_function() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let _sqlite_function_runtime = init_sqlite_function_runtime(&conn).await;

    let conn = trailbase_sqlite::Connection::from_connection_test_only(conn);
    let runtime = init_runtime(Some(conn.clone()));

    {
      // First call echo endpoint
      let resp = send_http_request(
        &runtime,
        "http://localhost:4000/sqlite_echo",
        "/sqlite_echo",
      )
      .await
      .unwrap();

      assert_eq!(5, response_to_i64(resp).await);
    }

    for i in 0..100 {
      let resp = send_http_request(
        &runtime,
        "http://localhost:4000/sqlite_stateful",
        "/sqlite_stateful",
      )
      .await
      .unwrap();

      assert_eq!(i, response_to_i64(resp).await);
    }
  }

  async fn send_http_request(
    runtime: &Runtime,
    uri: &str,
    registered_path: &str,
  ) -> Result<Response<UnsyncBoxBody<Bytes, ErrorCode>>, Error> {
    fn to_header_value(context: &HttpContext) -> hyper::http::HeaderValue {
      return hyper::http::HeaderValue::from_bytes(
        &serde_json::to_vec(&context).unwrap_or_default(),
      )
      .unwrap();
    }

    let uri = uri.to_string();
    let registered_path = registered_path.to_string();
    let context = HttpContext {
      kind: HttpContextKind::Http,
      registered_path,
      path_params: vec![],
      user: None,
    };

    let request = hyper::Request::builder()
      .uri(uri)
      .header("__context", to_header_value(&context))
      .body(sqlite::bytes_to_body(Bytes::from_static(b"")))
      .unwrap();

    let store = HttpStore::new(&runtime).await.unwrap();
    return store.call_incoming_http_handler(request).await;
  }

  async fn response_to_i64(resp: Response<UnsyncBoxBody<Bytes, ErrorCode>>) -> i64 {
    assert_eq!(resp.status(), StatusCode::OK, "{resp:?}");
    let body: Bytes = resp.into_body().collect().await.unwrap().to_bytes();
    return String::from_utf8_lossy(&body).trim().parse().unwrap();
  }
}
