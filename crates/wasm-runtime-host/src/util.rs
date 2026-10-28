use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use trailbase_wasi_keyvalue::WasiKeyValueCtx;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Result, Store};
use wasmtime_wasi::{FsPerms, WasiCtxBuilder};
use wasmtime_wasi_http::WasiHttpCtx;

use crate::Error;
use crate::host::{SharedState, State};

pub(crate) fn build_wasmtime_engine(use_winch: bool) -> Result<Engine, Error> {
  return Ok(Engine::new(&build_config(
    Some(wasmtime::Cache::new(wasmtime::CacheConfig::default())?),
    use_winch,
  ))?);
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

pub(crate) fn build_wasmtime_component(
  engine: &Engine,
  wasm_source_file: &Path,
) -> Result<Component, Error> {
  log::info!("Compiling: {wasm_source_file:?}. May take some time...");

  let start = SystemTime::now();
  let component = wasmtime::CodeBuilder::new(engine)
    .wasm_binary_or_text_file(wasm_source_file)?
    .compile_component()?;

  // NOTE: According to docs, this should not do anything (it seems like a reasonable thing to
  // call explicitly).
  component.initialize_copy_on_write_image()?;

  log::info!(
    "Loaded component {wasm_source_file:?} in: {elapsed:?}.",
    elapsed = SystemTime::now().duration_since(start).unwrap_or_default()
  );

  return Ok(component);
}

pub(crate) fn build_wasmtime_linker(engine: &Engine) -> Result<Linker<State>, Error> {
  let mut linker = Linker::<State>::new(engine);

  // Adds all the default WASI implementations: clocks, random, fs, ...
  wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;

  // Adds default HTTP interfaces - incoming and outgoing.
  wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker)?;

  // Add default KV interfaces.
  trailbase_wasi_keyvalue::add_to_linker(&mut linker, |cx| {
    trailbase_wasi_keyvalue::WasiKeyValue::new(&cx.kv, &mut cx.resource_table)
  })?;

  // Host interfaces.
  crate::host::trailbase::database::sqlite::add_to_linker::<_, State>(&mut linker, |s| s)?;

  return Ok(linker);
}

pub(crate) async fn initialize_bindings(
  linker: &Linker<State>,
  component: &Component,
  component_path: &PathBuf,
  store: &mut Store<State>,
) -> Result<crate::host::Interfaces, Error> {
  let instance_pre = linker.instantiate_pre(component).map_err(|err| {
    log::error!(
      "Failed to pre-instantiate WIT component {component_path:?}: '{err}'.\n{ABI_MISMATCH_WARNING}",
    );
    return err;
  })?;

  let instance = instance_pre
    .instantiate_async(&mut *store)
    .await
    .map_err(|err| {
      log::error!(
        "Failed to instantiate WIT component {component_path:?}: '{err}'.\n{ABI_MISMATCH_WARNING}",
      );
      return err;
    })?;

  return Ok(
    crate::host::Interfaces::new(store, &instance).map_err(|err| {
      log::error!("Failed to load WIT bindings for {component_path:?}: '{err}'.",);
      return err;
    })?,
  );
}

pub(crate) fn build_new_store(
  engine: &Engine,
  shared_state: &Arc<SharedState>,
  wasm_source_file: PathBuf,
) -> Result<Store<State>, Error> {
  let wasi_ctx = {
    let mut wasi_ctx = WasiCtxBuilder::new();
    wasi_ctx.inherit_stdio();
    wasi_ctx.stdin(wasmtime_wasi::p2::pipe::ClosedInputStream);
    // wasi_ctx.stdout(wasmtime_wasi::p2::Stdout);
    // wasi_ctx.stderr(wasmtime_wasi::p2::Stderr);

    wasi_ctx.args(&[""]);
    wasi_ctx.allow_tcp(false);
    wasi_ctx.allow_udp(false);
    wasi_ctx.allow_ip_name_lookup(true);

    if let Some(ref path) = shared_state.fs_root_path {
      wasi_ctx
        .preopened_dir(path, "/", FsPerms::ReadOnly)
        .map_err(|err| Error::Other(err.to_string()))?;
    }

    wasi_ctx.build()
  };

  return Ok(Store::new(
    engine,
    State {
      resource_table: ResourceTable::new(),
      wasi_ctx,
      http_ctx: WasiHttpCtx::new(),
      hooks: crate::host::Hooks {
        shared: shared_state.clone(),
        wasm_source_file,
      },
      kv: WasiKeyValueCtx::new(shared_state.kv_store.clone()),
      #[allow(deprecated)]
      tx: tokio::sync::Mutex::new(crate::host::TransactionImpl::default()),
      shared: shared_state.clone(),
    },
  ));
}

const ABI_MISMATCH_WARNING: &str = "\
    This may happen if the server and component are ABI incompatible. Make sure to run compatible \
    versions, i.e. update/rebuild the component to match the server binary or update your server \
    to run more up-to-date components.\n\
    First-party components can be updated easily by running `$ trail components update` or downloaded from: \
    https://github.com/trailbaseio/trailbase/releases.";
