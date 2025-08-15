#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use bytes::Bytes;
use futures_util::future::LocalBoxFuture;
use http_body_util::{BodyExt, combinators::BoxBody};
use std::rc::Rc;
use std::time::SystemTime;
use trailbase_schema::json::{JsonError, rich_json_to_value, value_to_rich_json};
use trailbase_wasm_common::{SqliteRequest, SqliteResponse};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Result, Store};
use wasmtime_wasi::p2::add_to_linker_async;
use wasmtime_wasi::p2::{IoView, WasiCtx, WasiCtxBuilder, WasiView};
use wasmtime_wasi::{DirPerms, FilePerms};
use wasmtime_wasi_http::bindings::http::types::ErrorCode;
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

use crate::exports::trailbase::runtime::init_endpoint::InitResult;

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
        // Ours:
        "wit/trailbase.wit",
    ],
    async: true,
    // Interactions with `ResourceTable` can possibly trap so enable the ability
    // to return traps from generated functions.
    trappable_imports: true,
});

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Wasmtime: {0}")]
  Wasmtime(#[from] wasmtime::Error),
  #[error("Channel closed")]
  ChannelClosed,
  #[error("Http Error: {0}")]
  HttpErrorCode(ErrorCode),
}

pub enum Message {
  Run(Box<dyn FnOnce(&RuntimeInstance) -> LocalBoxFuture<()> + Send>),
}

struct State {
  pub resource_table: ResourceTable,
  pub wasi_ctx: WasiCtx,
  pub http: WasiHttpCtx,

  pub conn: trailbase_sqlite::Connection,
}

impl IoView for State {
  fn table(&mut self) -> &mut ResourceTable {
    &mut self.resource_table
  }
}

impl WasiView for State {
  fn ctx(&mut self) -> &mut WasiCtx {
    &mut self.wasi_ctx
  }
}

impl WasiHttpView for State {
  fn ctx(&mut self) -> &mut WasiHttpCtx {
    &mut self.http
  }

  // NOTE: Based on `WasiView`' default implementation.
  fn send_request(
    &mut self,
    request: hyper::Request<wasmtime_wasi_http::body::HyperOutgoingBody>,
    config: wasmtime_wasi_http::types::OutgoingRequestConfig,
  ) -> wasmtime_wasi_http::HttpResult<wasmtime_wasi_http::types::HostFutureIncomingResponse> {
    println!("send_request {:?}: {request:?}", request.uri().host());

    return match request.uri().host() {
      Some("__sqlite") => {
        let conn = self.conn.clone();
        Ok(
          wasmtime_wasi_http::types::HostFutureIncomingResponse::pending(
            wasmtime_wasi::runtime::spawn(
              async move { Ok(handle_sqlite_request(conn, request).await) },
            ),
          ),
        )
      }
      _ => Ok(wasmtime_wasi_http::types::default_send_request(
        request, config,
      )),
    };
  }
}

fn err_mapper<E: std::error::Error>(err: E) -> ErrorCode {
  return ErrorCode::InternalError(Some(err.to_string()));
}

async fn handle_sqlite_request(
  conn: trailbase_sqlite::Connection,
  request: hyper::Request<wasmtime_wasi_http::body::HyperOutgoingBody>,
) -> Result<wasmtime_wasi_http::types::IncomingResponse, ErrorCode> {
  let (_parts, body) = request.into_parts();
  let bytes: Bytes = body
    .collect()
    .await
    .map_err(|_| ErrorCode::HttpRequestDenied)?
    .to_bytes();
  let sqlite_request: SqliteRequest =
    serde_json::from_slice(&bytes).map_err(|_| ErrorCode::HttpRequestDenied)?;

  let params = json_values_to_sqlite_params(sqlite_request.params).map_err(err_mapper)?;

  let rows = conn
    .write_query_rows(sqlite_request.query, params)
    .await
    .map_err(err_mapper)?;

  let values = rows
    .iter()
    .map(|row| -> Result<Vec<serde_json::Value>, ErrorCode> {
      return Ok(row_to_rich_json_array(row).map_err(err_mapper)?);
    })
    .collect::<Result<Vec<_>, _>>()
    .map_err(err_mapper)?;

  let body = serde_json::to_vec(&SqliteResponse {
    rows: values,
    error: None,
  })
  .map_err(err_mapper)?;

  let resp = http::Response::builder()
    .status(200)
    .body(bytes_to_body(Bytes::from_owner(body)))
    .map_err(err_mapper)?;

  return Ok(wasmtime_wasi_http::types::IncomingResponse {
    resp,
    worker: None,
    between_bytes_timeout: std::time::Duration::ZERO,
  });
}

pub struct Runtime {
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

impl Runtime {
  pub fn new(
    n_threads: usize,
    wasm_source_file: std::path::PathBuf,
    conn: trailbase_sqlite::Connection,
  ) -> Result<Self, Error> {
    let mut config = Config::new();
    config.async_support(true);
    config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
    let engine = Engine::new(&config)?;

    // Load the component.
    let component = {
      let start = SystemTime::now();
      let component = Component::from_file(&engine, &wasm_source_file)?;

      if let Ok(elapsed) = SystemTime::now().duration_since(start) {
        log::debug!("Component load in: {elapsed:?}");
      }
      component
    };

    let (shared_sender, shared_receiver) = kanal::unbounded_async::<Message>();
    let threads = (0..n_threads)
      .map(|index| -> Result<_, Error> {
        let (private_sender, private_receiver) = kanal::unbounded_async::<Message>();

        let shared_receiver = shared_receiver.clone();
        let instance = RuntimeInstance::new(engine.clone(), component.clone(), conn.clone())?;
        let handle = std::thread::spawn(move || {
          let tokio_runtime = Rc::new(
            tokio::runtime::Builder::new_current_thread()
              .enable_time()
              .enable_io()
              .thread_name(format!("wasm-runtime-{index}"))
              .build()
              .expect("startup"),
          );

          event_loop(tokio_runtime, instance, private_receiver, shared_receiver);
        });

        return Ok((handle, private_sender));
      })
      .collect::<Result<Vec<_>, Error>>()?;

    return Ok(Self {
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
          let _ = sender.send(f(runtime).await);
        })
      })))
      .await
      .map_err(|_| Error::ChannelClosed)?;

    return Ok(receiver.await.map_err(|_| Error::ChannelClosed)?);
  }
}

fn event_loop(
  tokio_runtime: Rc<tokio::runtime::Runtime>,
  instance: RuntimeInstance,
  private_recv: kanal::AsyncReceiver<Message>,
  shared_recv: kanal::AsyncReceiver<Message>,
) {
  let local = tokio::task::LocalSet::new();

  local.spawn_local(async move {
    loop {
      let receive_message = async || {
        return tokio::select! {
          msg = private_recv.recv() => msg,
          msg = shared_recv.recv() => msg,
        };
      };

      match receive_message().await {
        Ok(Message::Run(f)) => f(&instance).await,
        Err(_) => {
          // Channel closed
          return;
        }
      };
    }
  });

  tokio_runtime.block_on(local);
}

pub struct RuntimeInstance {
  engine: Engine,
  component: Component,
  linker: Linker<State>,

  conn: trailbase_sqlite::Connection,
}

impl RuntimeInstance {
  pub fn new(
    engine: Engine,
    component: Component,
    conn: trailbase_sqlite::Connection,
  ) -> Result<Self, Error> {
    let mut linker = Linker::new(&engine);

    // Adds all the default WASI implementations: clocks, random, fs, ...
    add_to_linker_async(&mut linker)?;

    // Adds default HTTP interfaces - incoming and outgoing.
    wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)?;

    return Ok(Self {
      engine,
      component,
      linker,
      conn,
    });
  }

  fn new_store(&self) -> Store<State> {
    let mut wasi_ctx = WasiCtxBuilder::new();
    wasi_ctx.inherit_stdio();
    wasi_ctx.args(&[""]);
    wasi_ctx.allow_tcp(false);
    wasi_ctx.allow_udp(false);
    wasi_ctx.allow_ip_name_lookup(true);

    if let Err(err) = wasi_ctx.preopened_dir(".", "/host", DirPerms::READ, FilePerms::READ) {
      log::error!("Failed to preopen dir: {err}");
    }

    return Store::new(
      &self.engine,
      State {
        resource_table: ResourceTable::new(),
        wasi_ctx: wasi_ctx.build(),
        http: WasiHttpCtx::new(),
        conn: self.conn.clone(),
      },
    );
  }

  pub async fn call_init(&self) -> Result<InitResult, Error> {
    let mut store = self.new_store();
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
    let mut store = self.new_store();
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
    let handle = wasmtime_wasi::runtime::spawn(async move {
      proxy
        .wasi_http_incoming_handler()
        .call_handle(&mut store, req, out)
        .await
    });

    let resp = match receiver.await {
      Ok(Ok(resp)) => Ok(resp),
      Ok(Err(err)) => Err(Error::HttpErrorCode(err)),
      Err(_) => {
        log::debug!("channel closed");
        Err(Error::ChannelClosed)
      }
    };

    // Now that the response has been processed, we can wait on the guest to
    // finish without deadlocking.
    handle.await?;

    return resp;
  }
}

#[inline]
fn bytes_to_body<E>(bytes: Bytes) -> BoxBody<Bytes, E> {
  BoxBody::new(http_body_util::Full::new(bytes).map_err(|_| unreachable!()))
}

fn json_values_to_sqlite_params(
  values: Vec<serde_json::Value>,
) -> Result<Vec<trailbase_sqlite::Value>, JsonError> {
  return values.into_iter().map(rich_json_to_value).collect();
}

pub fn row_to_rich_json_array(
  row: &trailbase_sqlite::Row,
) -> Result<Vec<serde_json::Value>, JsonError> {
  return (0..row.column_count())
    .map(|i| -> Result<serde_json::Value, JsonError> {
      let value = row.get_value(i).ok_or(JsonError::ValueNotFound)?;
      return value_to_rich_json(value);
    })
    .collect();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_init() {
    let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
    conn
      .execute_batch(
        "
        CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT);
        INSERT INTO test (value) VALUES ('test');
        ",
      )
      .await
      .unwrap();

    let runtime = Runtime::new(2, "./testdata/rust_guest.wasm".into(), conn).unwrap();

    runtime
      .call(async |instance| {
        instance.call_init().await.unwrap();
      })
      .await
      .unwrap();

    runtime
      .call(async |instance| {
        let request = hyper::Request::builder()
          .uri("https://www.rust-lang.org/")
          .body(bytes_to_body(Bytes::from_static(b"")))
          .unwrap();

        instance.call_incoming_http_handler(request).await.unwrap();
      })
      .await
      .unwrap();
  }
}
