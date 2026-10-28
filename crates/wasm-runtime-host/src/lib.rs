#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod functions;
mod host;
mod prefs;
mod sqlite;
mod util;

use bytes::Bytes;
use core::future::Future;
use deadpool::managed::Timeouts;
use http::Uri;
use http_body_util::combinators::UnsyncBoxBody;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tokio::task::JoinError;
use tokio::time::Duration;
use trailbase_wasm_common::manifest::InitManifest;
use wasmtime::component::{Component, Linker};
use wasmtime::{AsContextMut, Engine, Result, Store};
use wasmtime_wasi_http::WasiHttpView;
use wasmtime_wasi_http::p2::bindings::http::types::ErrorCode;

use crate::util::{
  build_new_store, build_wasmtime_component, build_wasmtime_engine, build_wasmtime_linker,
  initialize_bindings,
};

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
  #[error("Timeout: {0:?}")]
  Timeout(Option<Uri>),
  #[error("Other: {0}")]
  Other(String),
}

#[derive(Clone, Default, Debug)]
pub struct RuntimeOptions {
  /// Whether to use the non-optimizing baseline compiler.
  pub use_winch: bool,

  /// Which tokio runtime handle to execute on.
  pub tokio_runtime: Option<tokio::runtime::Handle>,
}

// NOTE: Tentatively generic to maybe split state between HttpStore and SqliteStore in the future
// :shrug:.
struct RuntimeInternal {
  /// Path to original .wasm component file.
  component_path: PathBuf,

  /// Wasmtime props.
  engine: Engine,
  linker: Linker<State>,
  component: Component,

  /// Our props
  shared_state: Arc<SharedState>,
  rt_handle: tokio::runtime::Handle,

  /// In-flight number of requests.
  local_in_flight: AtomicUsize,
}

impl Drop for RuntimeInternal {
  fn drop(&mut self) {
    log::debug!(
      "Dropped WASM runtime: {:?} (sqlite_functions={})",
      self.component_path,
      self.shared_state.conn.is_none()
    );
  }
}

/// Holds everything one needs to instantiate state for a component and run it, e.g. wasmtime
/// engine, linker, compiled component, state/store builder, etc.
#[derive(Clone)]
pub struct Runtime {
  state: Arc<RuntimeInternal>,
}

impl Runtime {
  pub fn init(
    wasm_source_file: impl Into<PathBuf>,
    shared_state: Arc<SharedState>,
    opts: RuntimeOptions,
  ) -> Result<Self, Error> {
    let engine = build_wasmtime_engine(opts.use_winch)?;
    let linker = build_wasmtime_linker(&engine)?;

    // Load the component - a very expensive operation generating code. Compilation happens in
    // parallel and will saturate the entire machine.
    let wasm_source_file = wasm_source_file.into();
    let component = build_wasmtime_component(&engine, &wasm_source_file)?;

    let rt_handle = opts.tokio_runtime.unwrap_or_else(|| {
      log::debug!("Re-using Tokio runtime from context");
      tokio::runtime::Handle::current()
    });

    let state = Arc::new(RuntimeInternal {
      engine,
      linker,
      component_path: wasm_source_file,
      component,
      shared_state,
      rt_handle,
      local_in_flight: AtomicUsize::new(0),
    });

    return Ok(Self { state });
  }

  pub fn component_path(&self) -> &Path {
    return &self.state.component_path;
  }

  async fn new_store_and_bindings(&self) -> Result<(Store<State>, crate::host::Interfaces), Error> {
    let mut store = build_new_store(
      &self.state.engine,
      &self.state.shared_state,
      self.state.component_path.clone(),
    )?;

    let bindings = initialize_bindings(
      &self.state.linker,
      &self.state.component,
      &self.state.component_path,
      &mut store,
    )
    .await?;

    return Ok((store, bindings));
  }
}

struct StoreAndBindings {
  store: Store<State>,
  proxy_bindings: wasmtime_wasi_http::p2::bindings::Proxy,
  // Can be used to mark an instance as defunct.
  has_trapped: bool,
}

struct StoreManager {
  rt: Runtime,
}

impl deadpool::managed::Manager for StoreManager {
  type Type = StoreAndBindings;
  type Error = Error;

  async fn create(&self) -> Result<StoreAndBindings, Error> {
    let (mut store, _bindings) = self.rt.new_store_and_bindings().await?;
    let proxy_bindings = wasmtime_wasi_http::p2::bindings::Proxy::instantiate_async(
      &mut store,
      &self.rt.state.component,
      &self.rt.state.linker,
    )
    .await?;
    return Ok(StoreAndBindings {
      store,
      proxy_bindings,
      has_trapped: false,
    });
  }

  async fn recycle(
    &self,
    candidate: &mut StoreAndBindings,
    metrics: &deadpool::managed::Metrics,
  ) -> Result<(), deadpool::managed::RecycleError<Error>> {
    if candidate.has_trapped {
      return Err(deadpool::managed::RecycleError::message("defunct"));
    }

    // Limit how often a store gets recycled to avoid persistent ballooning if guests have memory
    // leaks.
    if metrics.recycle_count > 2048 {
      return Err(deadpool::managed::RecycleError::message("count limit"));
    }
    if metrics.age().as_secs() > 3600 {
      return Err(deadpool::managed::RecycleError::message("age limit"));
    }

    return Ok(());
  }
}

enum HttpStoreInternal {
  // A state store is initialized per incoming request.
  Unique {
    rt: Runtime,
  },
  // A state store that is shared across incoming requests. We need a pool to:
  //  * Recursive self-requests don't deadlock on store acquisition.
  //  * Ensure devs cannot reliably rely on state sharing across requests.
  Shared {
    pool: deadpool::managed::Pool<StoreManager>,
  },
}

impl HttpStoreInternal {
  #[allow(unused)]
  fn component_path(&self) -> &Path {
    return match self {
      HttpStoreInternal::Unique { rt } => rt.component_path(),
      HttpStoreInternal::Shared { pool } => pool.manager().rt.component_path(),
    };
  }
}

pub struct InitArgs {
  pub version: Option<String>,
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
    let manifest = Self::call(rt, {
      let rt = rt.clone();
      async move {
        let (mut store, bindings) = rt.new_store_and_bindings().await?;
        let api = bindings.trailbase_component_init_endpoint();

        let args = serde_json::to_string(&trailbase_wasm_common::manifest::InitArguments {
          version: args.version.clone(),
          subsystems: Some(vec![
            trailbase_wasm_common::manifest::Subsystem::Metadata,
            trailbase_wasm_common::manifest::Subsystem::Http,
            trailbase_wasm_common::manifest::Subsystem::Jobs,
          ]),
        })?;

        store
          .run_concurrent(async |accessor| -> Result<InitManifest, Error> {
            let manifest_json = api
              .call_get_manifest(accessor, args)
              .await?
              .map_err(Error::Other)?;

            let manifest: trailbase_wasm_common::manifest::InitManifest =
              serde_json::from_str(&manifest_json)?;

            return Ok(manifest);
          })
          .await?
      }
    })
    .await
    .map_err(|join_err| Error::Other(join_err.to_string()))??;

    // NOTE: Sharing state between requests is more efficient at the cost of worse isolation.
    // However, most guest runtimes don't even support it :(. Especially jco does not.
    // We thus enable state sharing only for Rust guests :/.
    return match manifest.metadata.as_ref().and_then(|m| m.guest_runtime) {
      Some(trailbase_wasm_common::manifest::GuestRuntime::Rust) => {
        // We use a generous hard limit and then periodically prune the pool to a
        // smaller number of stand-by stores.
        use deadpool::managed::Timeouts;
        let pool = deadpool::managed::Pool::builder(StoreManager { rt: rt.clone() })
          .max_size(POOL_HARD_LIMIT)
          .runtime(deadpool::Runtime::Tokio1)
          .timeouts(Timeouts {
            wait: Some(DEFAULT_CALL_TIMEOUT),
            ..Default::default()
          })
          .build()
          .expect("deadpool construction");

        let weak = pool.weak();
        tokio::spawn(async move {
          const PERIOD: Duration = Duration::from_secs(60);
          loop {
            tokio::time::sleep(PERIOD).await;
            let Some(pool) = weak.upgrade() else {
              return;
            };

            // Keep all recently used stores but at least 16.
            const MIN_SIZE: usize = 16;
            const MAX_AGE: std::time::Duration = std::time::Duration::from_mins(2);

            let size_before = pool.status().size;
            if size_before > MIN_SIZE {
              let mut cnt = 0;
              pool.retain(|_, metrics| {
                cnt += 1;
                return cnt <= MIN_SIZE || metrics.last_used() < MAX_AGE;
              });
            }

            log::debug!(
              "periodic store-pool shrink '{name}': {size_before} => {size_after}",
              name = component_path_to_name(pool.manager().rt.component_path()).unwrap_or_default(),
              size_after = pool.status().size,
            );
          }
        });

        Ok((
          Self {
            state: Arc::new(HttpStoreInternal::Shared { pool }),
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
    timeout: Option<Duration>,
  ) -> Result<hyper::Response<wasmtime_wasi_http::p2::body::HyperOutgoingBody>, Error> {
    let rt = match &*self.state {
      HttpStoreInternal::Unique { rt } => rt,
      HttpStoreInternal::Shared { pool } => &pool.manager().rt,
    };

    return Self::call(rt, {
      let call_timeout = timeout.unwrap_or(DEFAULT_CALL_TIMEOUT);
      let state = self.state.clone();

      async move {
        let uri = request.uri().clone();
        let (sender, receiver) = tokio::sync::oneshot::channel::<
          Result<hyper::Response<wasmtime_wasi_http::p2::body::HyperOutgoingBody>, ErrorCode>,
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
        let handle = tokio::spawn(REQUEST_ID.scope(REQUEST_ID.with(|id| *id), async move {
          let uri = request.uri().clone();

          let dispatch = async || -> Result<(), Error> {
            match &*state {
              HttpStoreInternal::Unique { rt } => {
                // Instantiate a store per request.
                let (mut store, _bindings) = rt.new_store_and_bindings().await?;
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
                  .await?;
              }
              HttpStoreInternal::Shared { pool, .. } => {
                // Acquire shared store from pool.
                let StoreAndBindings {
                  ref mut store,
                  ref proxy_bindings,
                  ref mut has_trapped,
                } = *pool
                  .timeout_get(&Timeouts {
                    // NOTE: In principle we could wait shorter here, i.e. `call_timeout - dt`, the
                    // time we spent to get here. Typically `dt` should be small.
                    wait: Some(call_timeout),
                    ..Default::default()
                  })
                  .await
                  .map_err(|_err| Error::Timeout(None))?;

                debug_assert!(!*has_trapped);

                let req = store.data_mut().http().new_incoming_request(
                  wasmtime_wasi_http::p2::bindings::http::types::Scheme::Http,
                  request,
                )?;
                let out = store.data_mut().http().new_response_outparam(sender)?;

                proxy_bindings
                  .wasi_http_incoming_handler()
                  .call_handle(store.as_context_mut(), req, out)
                  .await
                  .map_err(|err| {
                    // Mark the pool entry as defunct so it won't get recyled.
                    *has_trapped = true;
                    return err;
                  })?;
              }
            };

            return Ok(());
          };

          let res = tokio::time::timeout(call_timeout, dispatch())
            .await
            .map_err(|_err| Error::Timeout(Some(uri)))
            .flatten();

          #[cfg(debug_assertions)]
          log::debug!(
            "HttpStore::wasi_http_incoming_handler() completed ({name}, id={id})",
            name = component_path_to_name(state.component_path()).unwrap_or_default(),
            id = REQUEST_ID.with(|id| *id),
          );

          return res;
        }));

        return tokio::select! {
          biased; // Enforces top to bottom order.

          response = receiver => {
            match response {
              Ok(maybe_response) => {
                // Got a proper response from the component via channel - all perfect.
                maybe_response.map_err(Error::HttpErrorCode)
              },
              Err(err) => {
                // Channel closed: this should not happen. Even if the component instance becomes
                // defunct, e.g. through a panic, the sender would be kept alive (keeping the
                // receiver open) and the error would be handled below via the dispatch handle.
                Err(Error::Other(format!("channel closed: {err}")))
              },
            }
          },
          dispatch = handle => {
            match dispatch {
              Ok(Ok(())) => {
                debug_assert!(false);
                // Response should have been delivered via receiver.
                Err(Error::Other("unreachable".to_string()))
              },
              Ok(Err(err)) => {
                // This can happen when a component instance becomes defunct, e.g. after panic/trap.
                // We need to get rid of the instance.
                Err(Error::Other(format!("dispatch failed: {err}")))
              },
              Err(err) => {
                // Tokio should not fail to spawn a new task for dispatch :/.
                Err(Error::Other(format!("spawn failed: {err}")))
              },
            }
          },
          // NOTE: We have a separate timeout here (besides the call timeout above), since
          // cancelling the call won't drop the sender to close the receiver (the sender is
          // leaked via the Wasmtime store). We thus have this additional timeout on the receiving
          // end.
          // It must not be shorter than the call timeout: guests only respond once a job
          // handler returns, so a shorter wait may report a still-running job as timed out.
          _timeout = tokio::time::sleep(call_timeout) => {
            Err(Error::Timeout(Some(uri)))
          },
        };
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

    let _local_in_flight = rt.state.local_in_flight.fetch_add(1, Ordering::Relaxed);
    let _in_flight = IN_FLIGHT.fetch_add(1, Ordering::Relaxed);

    #[cfg(debug_assertions)]
    log::debug!(
      "HttpStore::call() started: in flight (local={_local_in_flight}, global={_in_flight}) ({name}, id={id})",
      name = component_path_to_name(rt.component_path()).unwrap_or_default(),
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
          "HttpStore::call() completed: in flight (local={_local_in_flight}, global={_in_flight}) ({name}, id={id})",
          name = component_path_to_name(&rt_state.component_path).unwrap_or_default(),
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

/// Helper to derive intended use of winch based on dev mode.
pub fn use_winch(dev: bool) -> bool {
  return if cfg!(target_os = "macos") {
    // https://github.com/trailbaseio/trailbase/issues/206
    false
  } else {
    dev
  };
}

const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(20);
const POOL_HARD_LIMIT: usize = 65536;

#[cfg(test)]
mod tests {
  use super::*;

  use http::{Response, StatusCode};
  use http_body_util::{BodyExt, combinators::UnsyncBoxBody};
  use trailbase_wasm_common::{HttpContext, HttpContextKind};

  use crate::host::SharedState;

  const TEST_COMPONENT_PATH: &str = "../../client/testfixture/wasm/wasm_guest_testfixture.wasm";

  fn init_runtime(conn: Option<trailbase_sqlite::Connection>) -> Runtime {
    let shared_state = Arc::new(SharedState {
      conn,
      kv_store: KvStore::new(),
      db_path: ".".into(),
      fs_root_path: None,
    });

    return Runtime::init(
      &TEST_COMPONENT_PATH,
      shared_state,
      RuntimeOptions {
        ..Default::default()
      },
    )
    .unwrap();
  }

  #[tokio::test]
  async fn test_init() {
    let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
    let runtime = init_runtime(Some(conn.clone()));

    let (store, _manifest) = HttpStore::initialize(&runtime, InitArgs { version: None })
      .await
      .unwrap();

    let request = build_http_request("http://localhost:4000/transaction", "/transaction");
    let response = store
      .call_incoming_http_handler(request, None)
      .await
      .unwrap();

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
    let (sqlite_store, functions) =
      functions::init_sqlite_store_for_test(&TEST_COMPONENT_PATH).await;

    let conn = trailbase_sqlite::Connection::with_opts(
      move || -> Result<_, trailbase_sqlite::Error> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();

        functions::setup_connection(&conn, sqlite_store.clone(), &functions).unwrap();

        return Ok(conn);
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
  ) -> Result<Response<UnsyncBoxBody<Bytes, wasmtime_wasi_http::Error>>, Error> {
    let (store, _manifest) = HttpStore::initialize(&runtime, InitArgs { version: None })
      .await
      .unwrap();

    let request = build_http_request(uri, registered_path);
    return store.call_incoming_http_handler(request, None).await;
  }

  async fn response_to_i64(resp: Response<UnsyncBoxBody<Bytes, wasmtime_wasi_http::Error>>) -> i64 {
    let (head, body) = resp.into_parts();
    let body: Bytes = body.collect().await.unwrap().to_bytes();
    assert_eq!(head.status, StatusCode::OK, "{body:?}");
    return String::from_utf8_lossy(&body).trim().parse().unwrap();
  }
}
