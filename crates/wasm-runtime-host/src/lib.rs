#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod functions;
mod host;
mod prefs;
mod sqlite;

use bytes::Bytes;
use core::future::Future;
use http_body_util::combinators::UnsyncBoxBody;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::SystemTime;
use tokio::sync::Mutex;
use tokio::task::JoinError;
use trailbase_wasi_keyvalue::WasiKeyValueCtx;
use trailbase_wasm_common::manifest::InitManifest;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{AsContextMut, Config, Engine, Result, Store};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};
use wasmtime_wasi_http::WasiHttpCtx;
use wasmtime_wasi_http::p2::WasiHttpView;
use wasmtime_wasi_http::p2::bindings::http::types::ErrorCode;

use crate::host::TransactionImpl;

pub use crate::host::{SharedState, State};
pub use trailbase_wasi_keyvalue::Store as KvStore;
pub use trailbase_wasm_common::component_path_to_name;

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
  #[error("Json")]
  Json(#[from] serde_json::Error),
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
  fn new_store(&self, engine: &Engine, wasm_source_file: PathBuf) -> Result<Store<S>, Error>;
}

// NOTE: Tentatively generic to maybe split state between HttpStore and SqliteStore in the future
// :shrug:.
struct RuntimeInternal<T: StoreBuilder<State>> {
  engine: Engine,
  linker: Linker<State>,

  /// Path to original .wasm component file.
  component_path: PathBuf,
  component: Component,

  store_builder: T,

  rt_handle: tokio::runtime::Handle,
  local_in_flight: AtomicUsize,
}

/// Holds everything one needs to instantiate state for a component and run it, e.g. wasmtime
/// engine, linker, compiled component, state/store builder, etc.
#[derive(Clone)]
pub struct Runtime {
  state: Arc<RuntimeInternal<Arc<SharedState>>>,
}

impl Runtime {
  pub fn init(
    wasm_source_file: PathBuf,
    store_builder: Arc<SharedState>,
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
      wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker)?;

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
      component_path: wasm_source_file,
      component,
      store_builder,
      rt_handle,
      local_in_flight: AtomicUsize::new(0),
    });

    return Ok(Self { state });
  }

  pub fn component_path(&self) -> &Path {
    return &self.state.component_path;
  }

  async fn new_bindings(&self) -> Result<(Store<State>, crate::host::Interfaces), Error> {
    let mut store = self
      .state
      .store_builder
      .new_store(&self.state.engine, self.component_path().to_path_buf())?;

    let instance_pre = self
      .state
      .linker
      .instantiate_pre(&self.state.component)
      .map_err(|err| {
        log::error!(
          "Failed to pre-instantiate WIT component {path:?}: '{err}'.\n{ABI_MISMATCH_WARNING}",
          path = self.state.component_path
        );
        return err;
      })?;

    let instance = instance_pre
      .instantiate_async(&mut store)
      .await
      .map_err(|err| {
        log::error!(
          "Failed to instantiate WIT component {path:?}: '{err}'.\n{ABI_MISMATCH_WARNING}",
          path = self.state.component_path
        );
        return err;
      })?;

    let bindings = crate::host::Interfaces::new(&mut store, &instance).map_err(|err| {
      log::error!(
        "Failed to load WIT bindings for {path:?}: '{err}'.",
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

impl StoreBuilder<State> for Arc<SharedState> {
  fn new_store(&self, engine: &Engine, wasm_source_file: PathBuf) -> Result<Store<State>, Error> {
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
        http_ctx: WasiHttpCtx::new(),
        hooks: host::Hooks {
          shared: self.clone(),
          wasm_source_file,
        },
        kv: WasiKeyValueCtx::new(self.kv_store.clone()),
        #[allow(deprecated)]
        tx: tokio::sync::Mutex::new(TransactionImpl::default()),
        shared: self.clone(),
      },
    ));
  }
}

enum HttpStoreInternal {
  // A state store is initialized per incoming request.
  Unique {
    rt: Runtime,
  },
  // A state store that is shared across incoming requests.
  Shared {
    rt: Runtime,
    store: Mutex<Store<State>>,
    // bindings: crate::host::Interfaces,
    proxy_bindings: wasmtime_wasi_http::p2::bindings::Proxy,
  },
}

/// Main abstraction to send incoming HTTP requests into a guest "isolate" (provided runtime +
/// state). Due to the lack of proper async WASIp3 support, this is also used for Jobs etc.
/// Sync  There's also a separate sync SqliteStore "isolate".
#[derive(Clone)]
pub struct HttpStore {
  state: Arc<HttpStoreInternal>,
}

impl HttpStore {
  pub async fn initialize(rt: &Runtime, args: InitArgs) -> Result<(Self, InitManifest), Error> {
    let (mut store, _bindings, manifest) = Self::call(rt, {
      let rt = rt.clone();
      async move {
        let (mut store, bindings) = rt.new_bindings().await?;
        let api = bindings.trailbase_component_init_endpoint();

        let args = serde_json::to_string(&trailbase_wasm_common::manifest::InitArguments {
          version: args.version.clone(),
          subsystems: Some(vec![
            trailbase_wasm_common::manifest::Subsystem::Metadata,
            trailbase_wasm_common::manifest::Subsystem::Http,
            trailbase_wasm_common::manifest::Subsystem::Jobs,
          ]),
        })?;

        // let mut store = state.store.lock().await;
        let manifest = store
          .run_concurrent(async |accessor| -> Result<InitManifest, Error> {
            let manifest_json = api
              .call_get_manifest(accessor, args)
              .await?
              .map_err(Error::Other)?;

            let manifest: trailbase_wasm_common::manifest::InitManifest =
              serde_json::from_str(&manifest_json)?;

            return Ok(manifest);
          })
          .await??;

        Ok::<_, Error>((store, bindings, manifest))
      }
    })
    .await
    .map_err(|join_err| Error::Other(join_err.to_string()))??;

    // NOTE: Sharing state between requests is more efficient at the cost of worse isolation.
    // However, most guest runtimes don't even support it :(. Especially jco does not.
    // We thus enable state sharing only for Rust guests :/.
    return match manifest.metadata.as_ref().and_then(|m| m.guest_runtime) {
      Some(trailbase_wasm_common::manifest::GuestRuntime::Rust) => {
        let proxy_bindings = wasmtime_wasi_http::p2::bindings::Proxy::instantiate_async(
          &mut store,
          &rt.state.component,
          &rt.state.linker,
        )
        .await?;

        Ok((
          Self {
            state: Arc::new(HttpStoreInternal::Shared {
              rt: rt.clone(),
              store: Mutex::new(store),
              // bindings,
              proxy_bindings,
            }),
          },
          manifest,
        ))
      }
      _ => Ok((
        Self {
          state: Arc::new(HttpStoreInternal::Unique { rt: rt.clone() }),
        },
        manifest,
      )),
    };
  }

  /// Main entry-point for incoming HTTP requests. Typically called by an Axum handler.
  pub async fn call_incoming_http_handler(
    &self,
    request: hyper::Request<UnsyncBoxBody<Bytes, hyper::Error>>,
  ) -> Result<hyper::Response<wasmtime_wasi_http::p2::body::HyperOutgoingBody>, Error> {
    let rt = match &*self.state {
      HttpStoreInternal::Unique { rt } => rt,
      HttpStoreInternal::Shared { rt, .. } => rt,
    };

    return Self::call(rt, {
      let state = self.state.clone();
      async move {
        let (sender, receiver) = tokio::sync::oneshot::channel::<
          Result<hyper::Response<wasmtime_wasi_http::p2::body::HyperOutgoingBody>, ErrorCode>,
        >();

        let uri = request.uri().clone();

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
        let handle = tokio::spawn(REQUEST_ID.scope(REQUEST_ID.with(|id| *id), async move {
          let res = match &*state {
            HttpStoreInternal::Unique { rt } => {
              // Instantiate a store per request.
              let (mut store, _bindings) = rt.new_bindings().await?;
              let proxy_bindings = wasmtime_wasi_http::p2::bindings::Proxy::instantiate_async(
                &mut store,
                &rt.state.component,
                &rt.state.linker,
              )
              .await?;

              let req = store.data_mut().http().new_incoming_request(
                wasmtime_wasi_http::p2::bindings::http::types::Scheme::Http,
                request,
              )?;
              let out = store.data_mut().http().new_response_outparam(sender)?;
              proxy_bindings
                .wasi_http_incoming_handler()
                .call_handle(store.as_context_mut(), req, out)
                .await
            }
            HttpStoreInternal::Shared {
              store,
              proxy_bindings,
              ..
            } => {
              let uri = request.uri().clone();

              println!("Acquire: {uri}");
              let mut lock =
                tokio::time::timeout(tokio::time::Duration::from_secs(1), store.lock())
                  .await
                  .map_err(|err| {
                    log::error!("Acquisition TIMEOUT: {uri}");
                    return Error::Other(err.to_string());
                  })?;
              println!("Acquired: {uri}");

              let req = lock.data_mut().http().new_incoming_request(
                wasmtime_wasi_http::p2::bindings::http::types::Scheme::Http,
                request,
              )?;

              // FIXME: The issue is here: when the call gets stuck, the sender is
              // never freed and thus the receiver never completes or closes :/.
              let out = lock.data_mut().http().new_response_outparam(sender)?;
              let x = wasmtime::component::Resource::<_>::new_borrow(out.rep());

              let res = tokio::time::timeout(
                tokio::time::Duration::from_secs(1),
                proxy_bindings.wasi_http_incoming_handler().call_handle(
                  lock.as_context_mut(),
                  req,
                  x,
                ),
              )
              .await
              .map_err(|err| {
                lock.data_mut().resource_table.delete(out).unwrap();
                log::error!("call TIMEOUT: {uri}");
                return Error::Other(err.to_string());
              })?;

              println!("returned: {uri}");
              res
            }
          };

          #[cfg(debug_assertions)]
          log::debug!(
            "wasi_http_incoming_handler() completed call({id})",
            id = REQUEST_ID.with(|id| *id),
          );

          res
        }));

        match tokio::time::timeout(tokio::time::Duration::from_secs(4), receiver)
          .await
          .expect("receiver TIMEOUT")
        {
          Ok(Ok(resp)) => {
            println!("WOOOO {uri}");
            // NOTE: We cannot await the completion `call_handle` here with `handle.await?;`, since
            // we're not consuming the response body, see above.
            Ok(resp)
          }
          Ok(Err(err)) => {
            println!("HERE0 {uri}");
            handle
              .await
              .map_err(|err| Error::Other(err.to_string()))??;
            println!("HERE2");
            Err(Error::HttpErrorCode(err))
          }
          Err(_) => {
            println!("HERE4 {uri}");
            log::debug!("channel closed");
            handle
              .await
              .map_err(|err| Error::Other(err.to_string()))??;
            Err(Error::ChannelClosed)
          }
        }
      }
    })
    .await
    .map_err(|join_err| Error::Other(join_err.to_string()))?;
  }

  /// Wraps future to execute on the associated Tokio runtime and do some accounting/logging.
  fn call<'a, F>(
    rt: &'a Runtime,
    f: F,
  ) -> impl Future<Output = Result<F::Output, JoinError>> + use<'a, F>
  where
    F: Future + Send + 'static,
    F::Output: Send,
  {
    let id = REQUEST_ID_CNT.fetch_add(1, Ordering::Relaxed);

    let local_in_flight = rt.state.local_in_flight.fetch_add(1, Ordering::Relaxed);
    let in_flight = IN_FLIGHT.fetch_add(1, Ordering::Relaxed);

    #[cfg(debug_assertions)]
    log::debug!(
      "WASM runtime ({path:?}) `call({id})`. In flight (local={local_in_flight}, global={in_flight})",
      path = rt.component_path(),
    );

    // This is where we spawn a new task on the associated tokio runtime.
    return rt.state.rt_handle.spawn(REQUEST_ID.scope(id, {
      let rt_state = rt.state.clone();
      async move {
        let r = f.await;

        IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
        rt_state.local_in_flight.fetch_sub(1, Ordering::Relaxed);

        #[cfg(debug_assertions)]
        log::debug!(
          "WASM runtime ({path:?}) completed call({id})",
          path = rt_state.component_path,
          id = REQUEST_ID.with(|id| *id),
        );

        return r;
      }
    }));
  }
}

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
static REQUEST_ID_CNT: AtomicU64 = AtomicU64::new(0);

tokio::task_local! {
    pub(crate) static REQUEST_ID: u64;
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

      if metadata.is_file() || metadata.is_symlink() {
        let path = entry.path();
        if path.extension()? == "wasm" {
          return Some(path);
        }
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

    let (store, _manifest) = HttpStore::initialize(&runtime, InitArgs { version: None })
      .await
      .unwrap();

    let request = build_http_request("http://localhost:4000/transaction", "/transaction");
    let response = store.call_incoming_http_handler(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK, "{response:?}");

    assert_eq!(
      1,
      conn
        .read_query_row_get::<i64>("SELECT COUNT(*) FROM tx;", (), 0)
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
    let conn = parking_lot::Mutex::new(rusqlite::Connection::open_in_memory().ok());

    let _sqlite_function_runtime = {
      let lock = conn.lock();
      init_sqlite_function_runtime(lock.as_ref().unwrap()).await
    };

    let conn = trailbase_sqlite::Connection::with_opts(
      move || -> Result<_, rusqlite::Error> {
        // Consume the rusqlite connection, only works for one thread.
        let mut lock = conn.lock();
        return Ok(lock.take().unwrap());
      },
      trailbase_sqlite::Options {
        num_threads: Some(1),
        ..Default::default()
      },
    )
    .unwrap();

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

      // NOTE: The offset depends on what got initialized, e.g. http handlers, job handlers
      // or just sqlite functions.
      let offset = 0;
      assert_eq!(offset + i, response_to_i64(resp).await);
    }
  }

  fn build_http_request(
    uri: &str,
    registered_path: &str,
  ) -> hyper::Request<UnsyncBoxBody<Bytes, hyper::Error>> {
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

    return hyper::Request::builder()
      .uri(uri)
      .header("__context", to_header_value(&context))
      .body(sqlite::bytes_to_body(Bytes::from_static(b"")))
      .unwrap();
  }

  async fn send_http_request(
    runtime: &Runtime,
    uri: &str,
    registered_path: &str,
  ) -> Result<Response<UnsyncBoxBody<Bytes, ErrorCode>>, Error> {
    let (store, _manifest) = HttpStore::initialize(&runtime, InitArgs { version: None })
      .await
      .unwrap();

    let request = build_http_request(uri, registered_path);
    return store.call_incoming_http_handler(request).await;
  }

  async fn response_to_i64(resp: Response<UnsyncBoxBody<Bytes, ErrorCode>>) -> i64 {
    let (head, body) = resp.into_parts();
    let body: Bytes = body.collect().await.unwrap().to_bytes();
    assert_eq!(head.status, StatusCode::OK, "{body:?}");
    return String::from_utf8_lossy(&body).trim().parse().unwrap();
  }
}
