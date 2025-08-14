#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use bytes::Bytes;
use futures_util::future::LocalBoxFuture;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use std::rc::Rc;
use std::time::SystemTime;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Result, Store};
use wasmtime_wasi::p2::add_to_linker_async;
use wasmtime_wasi::p2::{IoView, WasiCtx, WasiCtxBuilder, WasiView};
use wasmtime_wasi::{DirPerms, FilePerms};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

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
  HttpErrorCode(wasmtime_wasi_http::bindings::http::types::ErrorCode),
}

pub type CallbackType = dyn (FnOnce(&RuntimeInstance) -> LocalBoxFuture<()>) + Send;

pub enum Message {
  Run(Box<CallbackType>),
}

struct State {
  pub resource_table: ResourceTable,
  pub wasi_ctx: WasiCtx,
  pub http: WasiHttpCtx,
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

pub async fn custom_send_request_handler(
  _request: hyper::Request<wasmtime_wasi_http::body::HyperOutgoingBody>,
  _config: wasmtime_wasi_http::types::OutgoingRequestConfig,
) -> Result<
  wasmtime_wasi_http::types::IncomingResponse,
  wasmtime_wasi_http::bindings::http::types::ErrorCode,
> {
  fn full(bytes: Bytes) -> wasmtime_wasi_http::body::HyperIncomingBody {
    BoxBody::new(http_body_util::Full::new(bytes).map_err(|_| unreachable!()))
  }

  let resp = http::Response::builder()
    .status(200)
    .body(full(Bytes::from_static(b"")))
    .map_err(|_| wasmtime_wasi_http::bindings::http::types::ErrorCode::ConnectionReadTimeout)?;

  return Ok(wasmtime_wasi_http::types::IncomingResponse {
    resp,
    worker: None,
    between_bytes_timeout: std::time::Duration::ZERO,
  });
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
    println!("send_request {:?}: {request:?}", request.uri().scheme());
    let scheme = request.uri().scheme();
    return match scheme.map(|s| s.as_str()) {
      // TODO: We could hack SQLite access with a custom `__db` scheme.
      Some("custom") => Ok(
        wasmtime_wasi_http::types::HostFutureIncomingResponse::pending(
          wasmtime_wasi::runtime::spawn(async move {
            Ok(custom_send_request_handler(request, config).await)
          }),
        ),
      ),
      _ => Ok(wasmtime_wasi_http::types::default_send_request(
        request, config,
      )),
    };
  }
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
  pub fn new(n_threads: usize, wasm_source_file: std::path::PathBuf) -> Result<Self, Error> {
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
    let threads: Vec<_> = (0..n_threads)
      .map(|index| -> Result<_, Error> {
        let (private_sender, private_receiver) = kanal::unbounded_async::<Message>();

        let shared_receiver = shared_receiver.clone();
        let instance = RuntimeInstance::new(engine.clone(), component.clone())?;
        let handle = std::thread::spawn(move || {
          let tokio_runtime = Rc::new(
            tokio::runtime::Builder::new_current_thread()
              .enable_time()
              .enable_io()
              .thread_name(format!("v8-runtime-{index}"))
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
    // Fut: Future<Output = O> + Send + 'static,
    O: Send + 'static,
  {
    let (sender, receiver) = tokio::sync::oneshot::channel::<O>();

    self
      .shared_sender
      .send(Message::Run(Box::new(move |runtime| {
        return Box::pin(async move {
          let _ = sender.send(f(runtime).await);
        });
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
}

impl RuntimeInstance {
  pub fn new(engine: Engine, component: Component) -> Result<Self, Error> {
    let mut linker = Linker::new(&engine);

    // Adds all the default WASI implementations: clocks, random, fs, ...
    add_to_linker_async(&mut linker)?;

    // Adds default HTTP interfaces - incoming and outgoing.
    wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)?;

    return Ok(Self {
      engine,
      component,
      linker,
    });
  }

  fn new_store(&self) -> Store<State> {
    let mut wasi_ctx = WasiCtxBuilder::new();
    wasi_ctx.inherit_stdio();
    wasi_ctx.args(&["bar"]);
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
      },
    );
  }

  pub async fn call_init(&self) -> Result<(), Error> {
    let mut store = self.new_store();
    let bindings = Trailbase::instantiate_async(&mut store, &self.component, &self.linker).await?;
    bindings
      .trailbase_runtime_init_endpoint()
      .call_init(&mut store)
      .await?;

    return Ok(());
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
      Result<
        hyper::Response<wasmtime_wasi_http::body::HyperOutgoingBody>,
        wasmtime_wasi_http::bindings::http::types::ErrorCode,
      >,
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

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_init() {
    let runtime = Runtime::new(2, "./testdata/rust_guest.wasm".into()).unwrap();

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
          .body(BoxBody::new(
            http_body_util::Full::new(Bytes::from_static(b"")).map_err(|_| unreachable!()),
          ))
          .unwrap();

        instance.call_incoming_http_handler(request).await.unwrap();
      })
      .await
      .unwrap();
  }
}
