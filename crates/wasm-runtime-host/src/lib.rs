#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

mod sqlite;

use bytes::Bytes;
use core::future::Future;
use futures_util::TryFutureExt;
use futures_util::future::LocalBoxFuture;
use http_body_util::combinators::BoxBody;
use parking_lot::Mutex;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;
use trailbase::runtime::host_endpoint::{TxError, Value};
use trailbase_sqlite::{Params, Rows};
use trailbase_wasi_keyvalue::WasiKeyValueCtx;
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Result, Store};
use wasmtime_wasi::p2::add_to_linker_async;
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxView};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};
use wasmtime_wasi_http::bindings::http::types::ErrorCode;
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};
use wasmtime_wasi_io::IoView;

use crate::exports::trailbase::runtime::init_endpoint::InitResult;

pub use trailbase_wasi_keyvalue::Store as KvStore;

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

// Documentation: https://docs.wasmtime.dev/api/wasmtime/component/macro.bindgen.html
wasmtime::component::bindgen!({
    world: "trailbase:runtime/trailbase",
    path: [
        // Order-sensitive: will import *.wit from the folder.
        "wit/deps-0.2.6/random",
        "wit/deps-0.2.6/io",
        "wit/deps-0.2.6/clocks",
        "wit/deps-0.2.6/filesystem",
        "wit/deps-0.2.6/sockets",
        "wit/deps-0.2.6/cli",
        "wit/deps-0.2.6/http",
        "wit/keyvalue-0.2.0-draft",
        // Ours:
        "wit/trailbase.wit",
    ],
    // NOTE: This doesn't seem to work even though it should be fixed:
    //   https://github.com/bytecodealliance/wasmtime/issues/10677
    // i.e. can't add db locks to shared state.
    require_store_data_send: false,
    // NOTE: Doesn't work: https://github.com/bytecodealliance/wit-bindgen/issues/812.
    // additional_derives: [
    //     serde::Deserialize,
    //     serde::Serialize,
    // ],
    // Interactions with `ResourceTable` can possibly trap so enable the ability
    // to return traps from generated functions.
    imports: {
        "trailbase:runtime/host-endpoint/tx-commit": trappable,
        "trailbase:runtime/host-endpoint/tx-rollback": trappable,
        "trailbase:runtime/host-endpoint/tx-execute": trappable,
        "trailbase:runtime/host-endpoint/tx-query": trappable,
        "trailbase:runtime/host-endpoint/thread-id": trappable,
        default: async | trappable,
    },
    exports: {
        default: async,
    },
});

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

pub enum Message {
  Run(Box<dyn FnOnce(Rc<RuntimeInstance>) -> LocalBoxFuture<'static, ()> + Send>),
}

#[derive(Clone)]
struct LockedTransaction(Rc<Mutex<Option<sqlite::OwnedTx>>>);

unsafe impl Send for LockedTransaction {}

struct State {
  resource_table: ResourceTable,
  wasi_ctx: WasiCtx,
  http: WasiHttpCtx,
  kv: WasiKeyValueCtx,

  shared: Arc<SharedState>,
  tx: LockedTransaction,
}

impl Drop for State {
  fn drop(&mut self) {
    #[cfg(debug_assertions)]
    if self.tx.0.lock().is_some() {
      log::warn!("pending transaction locking the DB");
    }
  }
}

impl IoView for State {
  fn table(&mut self) -> &mut ResourceTable {
    return &mut self.resource_table;
  }
}

impl WasiView for State {
  fn ctx(&mut self) -> WasiCtxView<'_> {
    return WasiCtxView {
      ctx: &mut self.wasi_ctx,
      table: &mut self.resource_table,
    };
  }
}

impl WasiHttpView for State {
  fn ctx(&mut self) -> &mut WasiHttpCtx {
    return &mut self.http;
  }

  fn table(&mut self) -> &mut ResourceTable {
    return &mut self.resource_table;
  }

  /// Receives HTTP fetches from the guest.
  ///
  /// Based on `WasiView`' default implementation.
  fn send_request(
    &mut self,
    request: hyper::Request<wasmtime_wasi_http::body::HyperOutgoingBody>,
    config: wasmtime_wasi_http::types::OutgoingRequestConfig,
  ) -> wasmtime_wasi_http::HttpResult<wasmtime_wasi_http::types::HostFutureIncomingResponse> {
    // log::debug!(
    //   "send_request {:?} {}: {request:?}",
    //   request.uri().host(),
    //   request.uri().path()
    // );

    return match request.uri().host() {
      Some("__sqlite") => {
        let conn = self.shared.conn.clone();
        Ok(
          wasmtime_wasi_http::types::HostFutureIncomingResponse::pending(
            wasmtime_wasi::runtime::spawn(async move {
              Ok(sqlite::handle_sqlite_request(conn, request).await)
            }),
          ),
        )
      }
      _ => {
        let handle = wasmtime_wasi::runtime::spawn(async move {
          Ok(wasmtime_wasi_http::types::default_send_request_handler(request, config).await)
        });
        Ok(wasmtime_wasi_http::types::HostFutureIncomingResponse::pending(handle))
      }
    };
  }
}

impl trailbase::runtime::host_endpoint::Host for State {
  fn thread_id(&mut self) -> wasmtime::Result<u64> {
    return Ok(self.shared.thread_id);
  }

  fn execute(
    &mut self,
    query: String,
    params: Vec<Value>,
  ) -> impl Future<Output = wasmtime::Result<Result<u64, TxError>>> + Send {
    let conn = self.shared.conn.clone();
    let params: Vec<_> = params.into_iter().map(to_sqlite_value).collect();

    return self
      .shared
      .runtime
      .spawn(async move {
        conn
          .execute(query, params)
          .await
          .map_err(|err| TxError::Other(err.to_string()))
          .map(|v| v as u64)
      })
      .map_err(|err| wasmtime::Error::msg(err.to_string()));
  }

  fn query(
    &mut self,
    query: String,
    params: Vec<Value>,
  ) -> impl Future<Output = wasmtime::Result<Result<Vec<Vec<Value>>, TxError>>> + Send {
    let conn = self.shared.conn.clone();
    let params: Vec<_> = params.into_iter().map(to_sqlite_value).collect();

    return self
      .shared
      .runtime
      .spawn(async move {
        let rows = conn
          .write_query_rows(query, params)
          .await
          .map_err(|err| TxError::Other(err.to_string()))?;

        let values: Vec<_> = rows
          .into_iter()
          .map(|trailbase_sqlite::Row(row, _col)| {
            return row.into_iter().map(from_sqlite_value).collect::<Vec<_>>();
          })
          .collect();

        Ok(values)
      })
      .map_err(|err| wasmtime::Error::msg(err.to_string()));
  }

  fn tx_begin(&mut self) -> impl Future<Output = wasmtime::Result<Result<(), TxError>>> + Send {
    async fn begin(
      conn: trailbase_sqlite::Connection,
      tx: LockedTransaction,
    ) -> Result<(), TxError> {
      assert!(tx.0.lock().is_none());

      *tx.0.lock() = Some(
        sqlite::new_tx(conn)
          .await
          .map_err(|err| TxError::Other(err.to_string()))?,
      );

      return Ok(());
    }

    let tx = self.tx.clone();
    return self
      .shared
      .runtime
      .spawn(begin(self.shared.conn.clone(), tx))
      .map_err(|err| wasmtime::Error::msg(err.to_string()));
  }

  fn tx_commit(&mut self) -> wasmtime::Result<Result<(), TxError>> {
    fn commit(tx: LockedTransaction) -> Result<(), TxError> {
      let Some(tx) = tx.0.lock().take() else {
        return Err(TxError::Other("no pending tx".to_string()));
      };

      // NOTE: this is the same as `tx.commit()` just w/o consuming.
      let lock = tx.borrow_dependent();
      lock
        .execute_batch("COMMIT")
        .map_err(|err| TxError::Other(err.to_string()))?;

      return Ok(());
    }

    return Ok(commit(self.tx.clone()));
  }

  fn tx_rollback(&mut self) -> wasmtime::Result<Result<(), TxError>> {
    fn rollback(tx: LockedTransaction) -> Result<(), TxError> {
      let Some(tx) = tx.0.lock().take() else {
        return Err(TxError::Other("no pending tx".to_string()));
      };

      // NOTE: this is the same as `tx.rollback()` just w/o consuming.
      let lock = tx.borrow_dependent();
      lock
        .execute_batch("ROLLBACK")
        .map_err(|err| TxError::Other(err.to_string()))?;

      return Ok(());
    }

    return Ok(rollback(self.tx.clone()));
  }

  fn tx_execute(
    &mut self,
    query: String,
    params: Vec<Value>,
  ) -> wasmtime::Result<Result<u64, TxError>> {
    fn execute(tx: LockedTransaction, query: String, params: Vec<Value>) -> Result<u64, TxError> {
      let params: Vec<_> = params.into_iter().map(to_sqlite_value).collect();

      let Some(ref tx) = *tx.0.lock() else {
        return Err(TxError::Other("No open transaction".to_string()));
      };

      let lock = tx.borrow_dependent();
      let mut stmt = lock
        .prepare(&query)
        .map_err(|err| TxError::Other(err.to_string()))?;

      params
        .bind(&mut stmt)
        .map_err(|err| TxError::Other(err.to_string()))?;

      return Ok(
        stmt
          .raw_execute()
          .map_err(|err| TxError::Other(err.to_string()))? as u64,
      );
    }

    return Ok(execute(self.tx.clone(), query, params));
  }

  fn tx_query(
    &mut self,
    query: String,
    params: Vec<Value>,
  ) -> wasmtime::Result<Result<Vec<Vec<Value>>, TxError>> {
    fn query_fn(
      tx: LockedTransaction,
      query: String,
      params: Vec<Value>,
    ) -> Result<Vec<Vec<Value>>, TxError> {
      let params: Vec<_> = params.into_iter().map(to_sqlite_value).collect();

      let Some(ref tx) = *tx.0.lock() else {
        return Err(TxError::Other("No open transaction".to_string()));
      };

      let lock = tx.borrow_dependent();
      let mut stmt = lock
        .prepare(&query)
        .map_err(|err| TxError::Other(err.to_string()))?;

      params
        .bind(&mut stmt)
        .map_err(|err| TxError::Other(err.to_string()))?;

      let rows =
        Rows::from_rows(stmt.raw_query()).map_err(|err| TxError::Other(err.to_string()))?;

      let values: Vec<_> = rows
        .into_iter()
        .map(|trailbase_sqlite::Row(row, _col)| {
          return row.into_iter().map(from_sqlite_value).collect::<Vec<_>>();
        })
        .collect();

      return Ok(values);
    }

    return Ok(query_fn(self.tx.clone(), query, params));
  }
}

pub struct Runtime {
  /// Path to original .wasm component file.
  pub component_path: std::path::PathBuf,

  // Shared sender.
  shared_sender: kanal::AsyncSender<Message>,
  threads: Vec<(std::thread::JoinHandle<()>, kanal::AsyncSender<Message>)>,
}

impl Drop for Runtime {
  fn drop(&mut self) {
    for (handle, ch) in std::mem::take(&mut self.threads) {
      // Dropping the private channel will trigger the event_loop to return.
      drop(ch);

      if let Err(err) = handle.join() {
        log::error!("Failed to join main rt thread: {err:?}");
      }
    }
  }
}

fn build_config(cache: Option<wasmtime::Cache>) -> Config {
  let mut config = Config::new();

  // Execution settings.
  config.async_support(true);
  config.epoch_interruption(false);
  config.memory_reservation(64 * 1024 * 1024 /* bytes */);
  // config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);

  // Compilation settings.
  config.cache(cache);
  config.cranelift_opt_level(wasmtime::OptLevel::Speed);
  config.parallel_compilation(true);

  return config;
}

#[derive(Clone, Default, Debug)]
pub struct RuntimeOptions {
  /// Number of threads the runtime will schedule on.
  pub n_threads: Option<usize>,

  /// Optional file-system sandbox root for r/o file access.
  pub fs_root_path: Option<std::path::PathBuf>,
}

impl Runtime {
  pub fn new(
    wasm_source_file: std::path::PathBuf,
    conn: trailbase_sqlite::Connection,
    kv_store: KvStore,
    opts: RuntimeOptions,
  ) -> Result<Self, Error> {
    let engine = Engine::new(&build_config(Some(wasmtime::Cache::new(
      wasmtime::CacheConfig::default(),
    )?)))?;

    // Load the component - a very expensive operation generating code. Compilation happens in
    // parallel and will saturate the entire machine.
    let component = {
      log::info!("Compiling: {wasm_source_file:?}. May take some time...");

      let start = SystemTime::now();
      let component = wasmtime::CodeBuilder::new(&engine)
        .wasm_binary_or_text_file(&wasm_source_file)?
        .compile_component()?;

      // NOTE: According to docs, this shouldn't do anything.
      component.initialize_copy_on_write_image()?;

      if let Ok(elapsed) = SystemTime::now().duration_since(start) {
        log::info!("Loaded component {wasm_source_file:?} in: {elapsed:?}.");
      }
      component
    };

    let linker = {
      let mut linker = Linker::<State>::new(&engine);

      // Adds all the default WASI implementations: clocks, random, fs, ...
      add_to_linker_async(&mut linker)?;

      // Adds default HTTP interfaces - incoming and outgoing.
      wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)?;

      // Add default KV interfaces.
      trailbase_wasi_keyvalue::add_to_linker(&mut linker, |cx| {
        trailbase_wasi_keyvalue::WasiKeyValue::new(&cx.kv, &mut cx.resource_table)
      })?;

      // Host interfaces.
      trailbase::runtime::host_endpoint::add_to_linker::<_, HasSelf<State>>(&mut linker, |s| s)?;

      linker
    };

    let n_threads = opts
      .n_threads
      .or(std::thread::available_parallelism().ok().map(|n| n.get()))
      .unwrap_or(1);

    log::info!("Starting WASM runtime with {n_threads} threads.");

    let (shared_sender, shared_receiver) = kanal::unbounded_async::<Message>();
    let threads = (0..n_threads)
      .map(|index| -> Result<_, Error> {
        let (private_sender, private_receiver) = kanal::unbounded_async::<Message>();

        let shared_receiver = shared_receiver.clone();

        let engine = engine.clone();
        let component = component.clone();
        let linker = linker.clone();

        let conn = conn.clone();
        let kv_store = kv_store.clone();
        let fs_root_path = opts.fs_root_path.clone();

        let handle = std::thread::Builder::new()
          .name(format!("wasm-runtime-{index}"))
          .spawn(move || {
            // Note: Arc rather than Rc, since State and thus SharedState needs to be Send + Sync.
            let tokio_runtime = tokio::runtime::Builder::new_current_thread()
              .enable_time()
              .enable_io()
              .build()
              .expect("startup");

            let shared_state = Arc::new(SharedState {
              runtime: tokio_runtime,
              conn,
              thread_id: index as u64,
              kv_store,
              fs_root_path,
            });

            let instance = RuntimeInstance {
              engine,
              component,
              linker,
              shared: shared_state.clone(),
            };
            // RuntimeInstance::new(engine, component, linker, shared_state).expect("startup");

            event_loop(shared_state, instance, private_receiver, shared_receiver);
          })
          .expect("failed to spawn thread");

        return Ok((handle, private_sender));
      })
      .collect::<Result<Vec<_>, Error>>()?;

    return Ok(Self {
      component_path: wasm_source_file,
      shared_sender,
      threads,
    });
  }

  pub async fn call<O, F>(&self, f: F) -> Result<O, Error>
  where
    F: (AsyncFnOnce(&RuntimeInstance) -> O) + Send + 'static,
    O: Send + 'static,
  {
    let (sender, receiver) = tokio::sync::oneshot::channel::<O>();

    self
      .shared_sender
      .send(Message::Run(Box::new(move |runtime| {
        Box::pin(async move {
          let _ = sender.send(f(&*runtime).await);
        })
      })))
      .await
      .map_err(|_| Error::ChannelClosed)?;

    return receiver.await.map_err(|_| Error::ChannelClosed);
  }
}

fn event_loop(
  shared_state: Arc<SharedState>,
  instance: RuntimeInstance,
  private_recv: kanal::AsyncReceiver<Message>,
  shared_recv: kanal::AsyncReceiver<Message>,
) {
  let thread_id = shared_state.thread_id;
  let local = tokio::task::LocalSet::new();
  let instance = Rc::new(instance);

  local.block_on(&shared_state.runtime, async move {
    let local_in_flight = Rc::new(AtomicUsize::new(0));

    loop {
      let receive_message = async || {
        return tokio::select! {
          msg = private_recv.recv() => msg,
          msg = shared_recv.recv() => msg,
        };
      };

      log::debug!(
        "Waiting for new messages (thread: {thread_id}). In flight: {}, {}",
        local_in_flight.load(Ordering::Relaxed),
        IN_FLIGHT.load(Ordering::Relaxed)
      );

      match receive_message().await {
        Ok(Message::Run(f)) => {
          let instance = instance.clone();

          let local_in_flight = local_in_flight.clone();
          local_in_flight.fetch_add(1, Ordering::Relaxed);

          IN_FLIGHT.fetch_add(1, Ordering::Relaxed);

          tokio::task::spawn_local(async move {
            f(instance).await;

            IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
            local_in_flight.fetch_sub(1, Ordering::Relaxed);
          });

          // Yield before listening for more messages to give JS a chance to run.
          tokio::task::yield_now().await;
        }
        Err(_) => {
          // Channel closed
          return;
        }
      };
    }
  });
}

pub struct SharedState {
  pub thread_id: u64,
  pub runtime: tokio::runtime::Runtime,
  pub conn: trailbase_sqlite::Connection,
  pub kv_store: KvStore,
  pub fs_root_path: Option<std::path::PathBuf>,
}

pub struct RuntimeInstance {
  engine: Engine,
  component: Component,
  linker: Linker<State>,

  shared: Arc<SharedState>,
}

impl RuntimeInstance {
  // pub fn new(
  //   engine: Engine,
  //   component: Component,
  //   linker: Linker<State>,
  //   shared_state: SharedState,
  // ) -> Result<Self, Error> {
  //   // let mut linker = Linker::<State>::new(&engine);
  //   //
  //   // // Adds all the default WASI implementations: clocks, random, fs, ...
  //   // add_to_linker_async(&mut linker)?;
  //   //
  //   // // Adds default HTTP interfaces - incoming and outgoing.
  //   // wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)?;
  //   //
  //   // // Add default KV interfaces.
  //   // trailbase_wasi_keyvalue::add_to_linker(&mut linker, |cx| {
  //   //   trailbase_wasi_keyvalue::WasiKeyValue::new(&cx.kv, &mut cx.resource_table)
  //   // })?;
  //   //
  //   // // Host interfaces.
  //   // trailbase::runtime::host_endpoint::add_to_linker::<_, HasSelf<State>>(&mut linker, |s|
  // s)?;
  //
  //   return Ok(Self {
  //     engine,
  //     component,
  //     linker,
  //     shared: Arc::new(shared_state),
  //   });
  // }

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
        tx: LockedTransaction(Rc::new(Mutex::new(None))),
      },
    ));
  }

  pub async fn call_init(&self) -> Result<InitResult, Error> {
    let mut store = self.new_store()?;
    let bindings = Trailbase::instantiate_async(&mut store, &self.component, &self.linker).await?;

    return Ok(
      bindings
        .trailbase_runtime_init_endpoint()
        .call_init(&mut store)
        .await?,
    );
  }

  pub async fn call_incoming_http_handler(
    &self,
    request: hyper::Request<BoxBody<Bytes, hyper::Error>>,
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
    let handle = self.shared.runtime.spawn(async move {
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

#[allow(unused)]
fn bytes_to_respone(
  bytes: Vec<u8>,
) -> Result<wasmtime_wasi_http::types::HostFutureIncomingResponse, ErrorCode> {
  let resp = http::Response::builder()
    .status(200)
    .body(sqlite::bytes_to_body(Bytes::from_owner(bytes)))
    .map_err(|err| ErrorCode::InternalError(Some(err.to_string())))?;

  return Ok(
    wasmtime_wasi_http::types::HostFutureIncomingResponse::ready(Ok(Ok(
      wasmtime_wasi_http::types::IncomingResponse {
        resp,
        worker: None,
        between_bytes_timeout: std::time::Duration::ZERO,
      },
    ))),
  );
}

fn to_sqlite_value(value: Value) -> trailbase_sqlite::Value {
  return match value {
    Value::Null => trailbase_sqlite::Value::Null,
    Value::Text(s) => trailbase_sqlite::Value::Text(s),
    Value::Real(f) => trailbase_sqlite::Value::Real(f),
    Value::Integer(i) => trailbase_sqlite::Value::Integer(i),
    Value::Blob(b) => trailbase_sqlite::Value::Blob(b),
  };
}

fn from_sqlite_value(value: trailbase_sqlite::Value) -> Value {
  return match value {
    trailbase_sqlite::Value::Null => Value::Null,
    trailbase_sqlite::Value::Text(s) => Value::Text(s),
    trailbase_sqlite::Value::Real(f) => Value::Real(f),
    trailbase_sqlite::Value::Integer(i) => Value::Integer(i),
    trailbase_sqlite::Value::Blob(b) => Value::Blob(b),
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  use http::{Response, StatusCode};
  use http_body_util::combinators::BoxBody;
  use trailbase_wasm_common::{HttpContext, HttpContextKind};

  #[tokio::test]
  async fn test_init() {
    let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
    let kv_store = KvStore::new();
    let runtime = Runtime::new(
      "../../client/testfixture/wasm/wasm_rust_guest_testfixture.wasm".into(),
      conn.clone(),
      kv_store,
      RuntimeOptions {
        n_threads: Some(2),
        ..Default::default()
      },
    )
    .unwrap();

    runtime
      .call(async |instance| {
        instance.call_init().await.unwrap();
      })
      .await
      .unwrap();

    let response = send_http_request(
      &runtime,
      "http://localhost:4000/transaction",
      "/transaction",
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

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
    let kv_store = KvStore::new();
    let runtime = Arc::new(
      Runtime::new(
        "../../client/testfixture/wasm/wasm_rust_guest_testfixture.wasm".into(),
        conn.clone(),
        kv_store,
        RuntimeOptions {
          n_threads: Some(2),
          ..Default::default()
        },
      )
      .unwrap(),
    );

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

  async fn send_http_request(
    runtime: &Runtime,
    uri: &str,
    registered_path: &str,
  ) -> Result<Response<BoxBody<Bytes, ErrorCode>>, Error> {
    fn to_header_value(context: &HttpContext) -> hyper::http::HeaderValue {
      return hyper::http::HeaderValue::from_bytes(
        &serde_json::to_vec(&context).unwrap_or_default(),
      )
      .unwrap();
    }

    let uri = uri.to_string();
    let registered_path = registered_path.to_string();
    return runtime
      .call(async |instance| {
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

        return instance.call_incoming_http_handler(request).await;
      })
      .await
      .unwrap();
  }
}
