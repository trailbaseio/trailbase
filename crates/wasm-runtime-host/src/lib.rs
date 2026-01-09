#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod functions;
mod host;
mod sqlite;

use bytes::Bytes;
use core::future::Future;
use http_body_util::combinators::UnsyncBoxBody;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;
use trailbase_wasi_keyvalue::WasiKeyValueCtx;
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Result, Store};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};
use wasmtime_wasi_http::bindings::http::types::ErrorCode;
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

use functions::ABI_MISMATCH_WARNING;
use host::exports::trailbase::component::init_endpoint::Arguments;
use host::{SharedState, State};

pub use host::exports::trailbase::component::init_endpoint::HttpMethodType;
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
}

pub struct Runtime {
  /// Path to original .wasm component file.
  rt_handle: tokio::runtime::Handle,
  local_in_flight: AtomicUsize,
  context: Arc<Context>,
}

impl Runtime {
  pub fn spawn(
    rt: Option<tokio::runtime::Handle>,
    wasm_source_file: PathBuf,
    conn: trailbase_sqlite::Connection,
    kv_store: KvStore,
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
      host::trailbase::database::sqlite::add_to_linker::<_, HasSelf<State>>(&mut linker, |s| s)?;

      linker
    };

    let rt_handle = rt.unwrap_or_else(|| {
      log::debug!("Re-using Tokio runtime from context");
      tokio::runtime::Handle::current()
    });

    let context = Arc::new(Context {
      engine: engine.clone(),
      component: component.clone(),
      linker: linker.clone(),
      shared: Arc::new(SharedState {
        conn: conn.clone(),
        kv_store: kv_store.clone(),
        fs_root_path: opts.fs_root_path.clone(),
        component_path: wasm_source_file,
      }),
    });

    return Ok(Self {
      rt_handle,
      local_in_flight: AtomicUsize::new(0),
      context,
    });
  }

  pub fn component_path(&self) -> &PathBuf {
    return &self.context.shared.component_path;
  }

  /// Call WASM component's `init` implementation.
  pub async fn initialize(&self, args: InitArgs) -> Result<InitResult, Error> {
    let context = self.context.clone();
    return self
      .call(async move { context.initialize(args).await })
      .await?;
  }

  /// Call http handlers exported by WASM component ("incoming" from the perspective of the
  /// component).
  pub async fn call_incoming_http_handler(
    &self,
    request: hyper::Request<UnsyncBoxBody<Bytes, hyper::Error>>,
  ) -> Result<hyper::Response<wasmtime_wasi_http::body::HyperOutgoingBody>, Error> {
    let context = self.context.clone();
    return self
      .call(async move { context.call_incoming_http_handler(request).await })
      .await?;
  }

  async fn call<F>(&self, f: F) -> Result<F::Output, Error>
  where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
  {
    #[cfg(debug_assertions)]
    log::debug!(
      "WASM runtime ({path:?}) waiting for new messages. In flight: {}, {}",
      self.local_in_flight.load(Ordering::Relaxed),
      IN_FLIGHT.load(Ordering::Relaxed),
      path = self.context.shared.component_path,
    );

    self.local_in_flight.fetch_add(1, Ordering::Relaxed);
    IN_FLIGHT.fetch_add(1, Ordering::Relaxed);

    let result = self.rt_handle.spawn(f).await;

    IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
    self.local_in_flight.fetch_sub(1, Ordering::Relaxed);

    return result.map_err(|_err| Error::ChannelClosed);
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

struct Context {
  engine: Engine,
  component: Component,
  linker: Linker<State>,

  shared: Arc<SharedState>,
}

impl Context {
  fn new_store(&self) -> Result<Store<State>, Error> {
    let mut wasi_ctx = WasiCtxBuilder::new();
    wasi_ctx.inherit_stdio();
    wasi_ctx.stdin(wasmtime_wasi::p2::pipe::ClosedInputStream);
    // wasi_ctx.stdout(wasmtime_wasi::p2::Stdout);
    // wasi_ctx.stderr(wasmtime_wasi::p2::Stderr);

    wasi_ctx.args(&[""]);
    wasi_ctx.allow_tcp(false);
    wasi_ctx.allow_udp(false);
    wasi_ctx.allow_ip_name_lookup(true);

    if let Some(ref path) = self.shared.fs_root_path {
      wasi_ctx
        .preopened_dir(path, "/", DirPerms::READ, FilePerms::READ)
        .map_err(|err| Error::Other(err.to_string()))?;
    }

    return Ok(Store::new(
      &self.engine,
      State {
        resource_table: ResourceTable::new(),
        wasi_ctx: wasi_ctx.build(),
        http: WasiHttpCtx::new(),
        kv: WasiKeyValueCtx::new(self.shared.kv_store.clone()),
        shared: self.shared.clone(),
        tx: Arc::new(Mutex::new(None)),
      },
    ));
  }

  async fn new_bindings(&self) -> Result<(Store<State>, crate::host::Interfaces), Error> {
    let mut store = self.new_store()?;

    let bindings =
      crate::host::Interfaces::instantiate_async(&mut store, &self.component, &self.linker)
        .await
        .map_err(|err| {
          log::error!(
            "Failed to instantiate WIT component {path:?}: '{err}'.\n{ABI_MISMATCH_WARNING}",
            path = self.shared.component_path
          );
          return err;
        })?;

    return Ok((store, bindings));
  }

  async fn initialize(&self, args: InitArgs) -> Result<InitResult, Error> {
    let (mut store, bindings) = self.new_bindings().await?;
    let api = bindings.trailbase_component_init_endpoint();

    let args = Arguments {
      version: args.version,
    };

    return Ok(InitResult {
      http_handlers: api
        .call_init_http_handlers(&mut store, &args)
        .await?
        .handlers,
      job_handlers: api
        .call_init_job_handlers(&mut store, &args)
        .await?
        .handlers,
    });
  }

  async fn call_incoming_http_handler(
    &self,
    request: hyper::Request<UnsyncBoxBody<Bytes, hyper::Error>>,
  ) -> Result<hyper::Response<wasmtime_wasi_http::body::HyperOutgoingBody>, Error> {
    let mut store = self.new_store()?;

    let proxy = wasmtime_wasi_http::bindings::Proxy::instantiate_async(
      &mut store,
      &self.component,
      &self.linker,
    )
    .await?;

    let req = store.data_mut().new_incoming_request(
      wasmtime_wasi_http::bindings::http::types::Scheme::Http,
      request,
    )?;

    let (sender, receiver) = tokio::sync::oneshot::channel::<
      Result<hyper::Response<wasmtime_wasi_http::body::HyperOutgoingBody>, ErrorCode>,
    >();

    let out = store.data_mut().new_response_outparam(sender)?;

    // NOTE: wstd streams out responses in chunks of 2kB. Only once everything has been streamed,
    // `call_handle` will complete. This is also when the streaming response body completes.
    //
    // We cannot use `wasmtime_wasi::runtime::spawn` here, which aborts the call when the handle
    // gets dropped, since we're not awaiting the response stream here. We'd either have to consume
    // the entire response here, keep the handle alive or as we currently do use a non-aborting
    // spawn.
    //
    // In the current setup, if the listening side hangs-up the they call may not be aborted.
    // Depends on what the implementation does when the streaming body's receiving end gets
    // out of scope.
    let handle = tokio::spawn(async move {
      proxy
        .wasi_http_incoming_handler()
        .call_handle(&mut store, req, out)
        .await
    });

    return match receiver.await {
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
    };
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
  // NOTE: This is where we enable async execution. Ironically, this runtime setting requires
  // compile-time setting to make all guest-exported bindings async... *all*. With this enabled
  // calling syncronous bindings will panic.
  config.async_support(true);
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

#[cfg(test)]
mod tests {
  use super::*;

  use http::{Response, StatusCode};
  use http_body_util::{BodyExt, combinators::UnsyncBoxBody};
  use trailbase_wasm_common::{HttpContext, HttpContextKind};

  const WASM_COMPONENT_PATH: &str = "../../client/testfixture/wasm/wasm_guest_testfixture.wasm";

  fn spawn_runtime(conn: trailbase_sqlite::Connection) -> Runtime {
    return Runtime::spawn(
      None,
      WASM_COMPONENT_PATH.into(),
      conn.clone(),
      KvStore::new(),
      RuntimeOptions {
        ..Default::default()
      },
    )
    .unwrap();
  }

  fn init_sqlite_function_runtime(conn: &rusqlite::Connection) -> functions::SqliteFunctionRuntime {
    let runtime = functions::SqliteFunctionRuntime::new(
      WASM_COMPONENT_PATH.into(),
      RuntimeOptions {
        ..Default::default()
      },
    )
    .unwrap();

    let functions = runtime
      .initialize_sqlite_functions(InitArgs { version: None })
      .unwrap();

    functions::setup_connection(conn, &runtime, &functions).unwrap();

    return runtime;
  }

  #[tokio::test]
  async fn test_init() {
    let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
    let runtime = spawn_runtime(conn.clone());

    runtime
      .initialize(InitArgs { version: None })
      .await
      .unwrap();

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
    let runtime = Arc::new(spawn_runtime(conn.clone()));

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
    let _sqlite_function_runtime = init_sqlite_function_runtime(&conn);

    let conn = trailbase_sqlite::Connection::from_connection_test_only(conn);
    let runtime = spawn_runtime(conn.clone());

    let response = send_http_request(&runtime, "http://localhost:4000/custom_fun", "/custom_fun")
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::OK, "{response:?}");

    let body: Bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.to_vec(), b"5\n");
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

    return runtime.call_incoming_http_handler(request).await;
  }
}
