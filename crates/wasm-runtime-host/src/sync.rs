use bytes::Bytes;
use core::future::Future;
use futures_util::future::BoxFuture;
use http_body_util::combinators::BoxBody;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;
use trailbase_wasi_keyvalue::Store as KvStore;
use trailbase_wasi_keyvalue::WasiKeyValueCtx;
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Result, Store};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxView};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};
use wasmtime_wasi_http::bindings::http::types::ErrorCode;
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};
use wasmtime_wasi_io::IoView;

use crate::Error;
use crate::RuntimeOptions;

// Experiment: re-exporting the above bindings as sync.
wasmtime::component::bindgen!({
    world: "trailbase:component/init",
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
        // // Ours:
        "wit/trailbase/database",
        "wit/trailbase/component",
    ],
    // NOTE: This doesn't seem to work even though it should be fixed:
    //   https://github.com/bytecodealliance/wasmtime/issues/10677
    // i.e. can't add db locks to shared state.
    require_store_data_send: false,
    imports: {
        default: trappable,
    },
    exports: {
        default: trappable,
    },
});

fn build_sync_config(cache: Option<wasmtime::Cache>, use_winch: bool) -> Config {
  let mut config = Config::new();

  // Execution settings:
  config.epoch_interruption(false);
  config.memory_reservation(64 * 1024 * 1024 /* bytes */);
  // NOTE: This is where we enable async execution. Ironically, this runtime setting requires
  // compile-time setting to make all guest-exported bindings async... *all*. With this enabled
  // calling syncronous bindings will panic.
  config.async_support(false);
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

struct State {
  resource_table: ResourceTable,
  wasi_ctx: WasiCtx,
  http: WasiHttpCtx,
  kv: WasiKeyValueCtx,
  // shared: Arc<SharedState>,
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
}

use self::trailbase::database::sqlite::{TxError, Value};

impl self::trailbase::database::sqlite::Host for State {
  fn execute(
    &mut self,
    query: String,
    params: Vec<Value>,
  ) -> wasmtime::Result<Result<u64, TxError>> {
    return Err(wasmtime::Error::msg(""));
  }

  fn query(
    &mut self,
    query: String,
    params: Vec<Value>,
  ) -> wasmtime::Result<Result<Vec<Vec<Value>>, TxError>> {
    return Err(wasmtime::Error::msg(""));
  }

  fn tx_begin(&mut self) -> wasmtime::Result<Result<(), TxError>> {
    return Err(wasmtime::Error::msg(""));
  }

  fn tx_commit(&mut self) -> wasmtime::Result<Result<(), TxError>> {
    return Err(wasmtime::Error::msg(""));
  }

  fn tx_rollback(&mut self) -> wasmtime::Result<Result<(), TxError>> {
    return Err(wasmtime::Error::msg(""));
  }

  fn tx_execute(
    &mut self,
    query: String,
    params: Vec<Value>,
  ) -> wasmtime::Result<Result<u64, TxError>> {
    return Err(wasmtime::Error::msg(""));
  }

  fn tx_query(
    &mut self,
    query: String,
    params: Vec<Value>,
  ) -> wasmtime::Result<Result<Vec<Vec<Value>>, TxError>> {
    return Err(wasmtime::Error::msg(""));
  }
}

pub struct SyncRuntimeInstance {
  engine: Engine,
  component: Component,
  linker: Linker<State>,
}

impl SyncRuntimeInstance {
  pub fn new(wasm_source_file: std::path::PathBuf, opts: RuntimeOptions) -> Result<Self, Error> {
    let engine = {
      let cache = wasmtime::Cache::new(wasmtime::CacheConfig::default())?;
      let config = build_sync_config(Some(cache), opts.use_winch);

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
      wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
      //
      // // Adds default HTTP interfaces - incoming and outgoing.
      wasmtime_wasi_http::add_only_http_to_linker_sync(&mut linker)?;

      // // Add default KV interfaces.
      trailbase_wasi_keyvalue::add_to_linker(&mut linker, |cx| {
        trailbase_wasi_keyvalue::WasiKeyValue::new(&cx.kv, &mut cx.resource_table)
      })?;

      // Host interfaces.
      trailbase::database::sqlite::add_to_linker::<_, HasSelf<State>>(&mut linker, |s| s)?;

      linker
    };

    // let (shared_sender, shared_receiver) = kanal::unbounded_async::<Message>();
    let instance = SyncRuntimeInstance {
      engine: engine.clone(),
      component: component.clone(),
      linker: linker.clone(),
      // shared: Arc::new(SharedState {
      //   conn: conn.clone(),
      //   kv_store: kv_store.clone(),
      //   fs_root_path: opts.fs_root_path.clone(),
      // }),
    };

    return Ok(instance);
  }

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

    // if let Some(ref path) = self.shared.fs_root_path {
    //   wasi_ctx
    //     .preopened_dir(path, "/", DirPerms::READ, FilePerms::READ)
    //     .map_err(|err| Error::Other(err.to_string()))?;
    // }

    let kv_store = KvStore::new();

    return Ok(Store::new(
      &self.engine,
      State {
        resource_table: ResourceTable::new(),
        wasi_ctx: wasi_ctx.build(),
        http: WasiHttpCtx::new(),
        kv: WasiKeyValueCtx::new(kv_store.clone()),
        // kv: WasiKeyValueCtx::new(self.shared.kv_store.clone()),
        // shared: self.shared.clone(),
        // tx: Arc::new(Mutex::new(None)),
      },
    ));
  }
}

#[cfg(test)]
mod tests {
  use super::exports::trailbase::component::init_endpoint::Arguments;
  use super::*;

  const WASM_COMPONENT_PATH: &str = "../../client/testfixture/wasm/wasm_guest_testfixture.wasm";

  #[tokio::test]
  async fn test_init() {
    let runtime = SyncRuntimeInstance::new(
      WASM_COMPONENT_PATH.into(),
      RuntimeOptions {
        ..Default::default()
      },
    )
    .unwrap();

    let mut store = runtime.new_store().unwrap();

    let bindings = Init::instantiate(&mut store, &runtime.component, &runtime.linker)
      .map_err(|err| {
        log::error!(
          "Failed to instantiate WIT component: '{err}'.\n This may happen if the server and \
           component are ABI incompatible. Make sure to run compatible versions, e.g. update your \
           server to run more recent components or rebuild your component against a more recent, \
           matching runtime."
        );
        return err;
      })
      .unwrap();

    let api = bindings.trailbase_component_init_endpoint();

    let args = Arguments { version: None };

    api.call_init_http_handlers(&mut store, &args).unwrap();
  }
}
