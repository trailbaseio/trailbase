use bytes::Bytes;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
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
    .map_err(|_| wasmtime_wasi_http::bindings::http::types::ErrorCode::ConnectionReadTimeout)
    .unwrap();

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

async fn foo() -> Result<()> {
  let wasm_source_file = std::env::args()
    .nth(1)
    .unwrap_or("target/wasm32-wasip2/debug/rust_guest.wasm".to_string());

  // Construct the wasm engine with async support enabled.
  let mut config = Config::new();
  config.async_support(true);
  config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
  let engine = Engine::new(&config)?;

  // Load the component.
  let component = {
    let start = SystemTime::now();
    let component = Component::from_file(&engine, &wasm_source_file)?;
    println!(
      "Component load in: {:?}",
      SystemTime::now().duration_since(start).unwrap()
    );
    component
  };

  let linker = {
    let mut linker = Linker::new(&engine);

    // Adds all the default implementations: clocks, random, filesystem, ...
    add_to_linker_async(&mut linker)?;

    // Adds default HTTP handling - incoming and outgoing.
    wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)?;

    linker
  };

  let mut store = Store::new(
    &engine,
    State {
      wasi_ctx: WasiCtxBuilder::new()
        .inherit_stdio()
        .args(&["bar"])
        .allow_tcp(false)
        .allow_udp(false)
        .allow_ip_name_lookup(true)
        .preopened_dir(".", "/host", DirPerms::READ, FilePerms::READ)
        .unwrap()
        .build(),
      resource_table: ResourceTable::new(),
      http: WasiHttpCtx::new(),
    },
  );

  let bindings = Trailbase::instantiate_async(&mut store, &component, &linker).await?;

  bindings
    .trailbase_runtime_init_endpoint()
    .call_init(&mut store)
    .await?;

  return Ok(());
}
