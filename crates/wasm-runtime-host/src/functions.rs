use std::time::SystemTime;
use trailbase_wasi_keyvalue::Store as KvStore;
use trailbase_wasi_keyvalue::WasiKeyValueCtx;
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Engine, Result, Store};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi_http::WasiHttpCtx;

use crate::Error;
use crate::RuntimeOptions;

mod sync {
  use trailbase_wasi_keyvalue::WasiKeyValueCtx;
  use wasmtime::component::ResourceTable;
  use wasmtime::{Config, Result};
  use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};
  use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

  use self::trailbase::database::sqlite::{TxError, Value};

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

  pub(super) fn build_config(cache: Option<wasmtime::Cache>, use_winch: bool) -> Config {
    let mut config = crate::build_config(cache, use_winch);
    config.async_support(false);
    return config;
  }

  pub(super) struct State {
    pub resource_table: ResourceTable,
    pub wasi_ctx: WasiCtx,
    pub http: WasiHttpCtx,
    pub kv: WasiKeyValueCtx,
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

  impl self::trailbase::database::sqlite::Host for State {
    fn tx_begin(&mut self) -> wasmtime::Result<Result<(), TxError>> {
      return Err(wasmtime::Error::msg("not implemented"));
    }

    fn tx_commit(&mut self) -> wasmtime::Result<Result<(), TxError>> {
      return Err(wasmtime::Error::msg("not implemented"));
    }

    fn tx_rollback(&mut self) -> wasmtime::Result<Result<(), TxError>> {
      return Err(wasmtime::Error::msg("not implemented"));
    }

    fn tx_execute(
      &mut self,
      _query: String,
      _params: Vec<Value>,
    ) -> wasmtime::Result<Result<u64, TxError>> {
      return Err(wasmtime::Error::msg("not implemented"));
    }

    fn tx_query(
      &mut self,
      _query: String,
      _params: Vec<Value>,
    ) -> wasmtime::Result<Result<Vec<Vec<Value>>, TxError>> {
      return Err(wasmtime::Error::msg("not implemented"));
    }
  }
}

pub use sync::exports::trailbase::component::sqlite_function_endpoint::Value;

#[derive(Clone)]
pub struct SqliteFunctionRuntime {
  /// Path to original .wasm component file.
  component_path: std::path::PathBuf,

  engine: Engine,
  component: Component,
  linker: Linker<sync::State>,
}

pub struct SqliteScalarFunction {
  pub name: String,
  pub num_args: u32,
  pub flags: Vec<rusqlite::functions::FunctionFlags>,
}

pub struct SqliteFunctions {
  pub scalar_functions: Vec<SqliteScalarFunction>,
}

impl SqliteFunctionRuntime {
  pub fn new(wasm_source_file: std::path::PathBuf, opts: RuntimeOptions) -> Result<Self, Error> {
    let engine = {
      let cache = wasmtime::Cache::new(wasmtime::CacheConfig::default())?;
      let config = sync::build_config(Some(cache), opts.use_winch);

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
      let mut linker = Linker::<sync::State>::new(&engine);

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
      sync::trailbase::database::sqlite::add_to_linker::<_, HasSelf<sync::State>>(
        &mut linker,
        |s| s,
      )?;

      linker
    };

    let instance = SqliteFunctionRuntime {
      component_path: wasm_source_file,
      engine,
      component,
      linker,
    };

    return Ok(instance);
  }

  fn new_store(&self) -> Result<Store<sync::State>, Error> {
    let mut wasi_ctx = WasiCtxBuilder::new();
    wasi_ctx.inherit_stdio();
    wasi_ctx.stdin(wasmtime_wasi::p2::pipe::ClosedInputStream);
    // wasi_ctx.stdout(wasmtime_wasi::p2::Stdout);
    // wasi_ctx.stderr(wasmtime_wasi::p2::Stderr);

    wasi_ctx.args(&[""]);
    wasi_ctx.allow_tcp(false);
    wasi_ctx.allow_udp(false);
    wasi_ctx.allow_ip_name_lookup(true);

    return Ok(Store::new(
      &self.engine,
      sync::State {
        resource_table: ResourceTable::new(),
        wasi_ctx: wasi_ctx.build(),
        http: WasiHttpCtx::new(),
        kv: WasiKeyValueCtx::new(KvStore::new()),
      },
    ));
  }

  fn new_bindings(&self) -> Result<(Store<sync::State>, sync::Init), Error> {
    let mut store = self.new_store()?;

    let bindings =
      sync::Init::instantiate(&mut store, &self.component, &self.linker).map_err(|err| {
        log::error!(
          "Failed to instantiate WIT component {path:?}: '{err}'.\n{ABI_MISMATCH_WARNING}",
          path = self.component_path
        );
        return err;
      })?;

    return Ok((store, bindings));
  }

  // Call WASM components `init` implementation.
  pub fn initialize_sqlite_functions(
    &self,
    args: crate::InitArgs,
  ) -> Result<SqliteFunctions, Error> {
    let (mut store, bindings) = self.new_bindings()?;
    let api = bindings.trailbase_component_init_endpoint();

    let args = sync::exports::trailbase::component::init_endpoint::Arguments {
      version: args.version,
    };

    return Ok(SqliteFunctions {
      scalar_functions: api
        .call_init_sqlite_functions(&mut store, &args)?
        .scalar_functions
        .into_iter()
        .map(|f| {
          return SqliteScalarFunction {
            name: f.name,
            num_args: f.num_args,
            flags: f
              .function_flags
              .into_iter()
              .map(|f| -> rusqlite::functions::FunctionFlags {
                return rusqlite::functions::FunctionFlags::from_bits_truncate(f as i32);
              })
              .collect(),
          };
        })
        .collect(),
    });
  }

  pub fn dispatch_scalar_function(
    &self,
    function_name: String,
    args: Vec<Value>,
  ) -> Result<Value, Error> {
    use sync::exports::trailbase::component::sqlite_function_endpoint::Arguments;

    let (mut store, bindings) = self.new_bindings()?;
    let api = bindings.trailbase_component_sqlite_function_endpoint();

    let args = Arguments {
      function_name,
      arguments: args,
    };

    return api
      .call_dispatch_scalar_function(&mut store, &args)?
      .map_err(|err| {
        return Error::Other(err.to_string());
      });
  }
}

pub fn setup_connection(
  conn: &rusqlite::Connection,
  runtime: &SqliteFunctionRuntime,
  functions: &SqliteFunctions,
) -> Result<(), rusqlite::Error> {
  for function in &functions.scalar_functions {
    let rt = runtime.clone();
    let function_name = function.name.clone();

    let flags = {
      if function.flags.is_empty() {
        rusqlite::functions::FunctionFlags::default()
      } else {
        let mut flags = rusqlite::functions::FunctionFlags::from_bits_truncate(0);
        for flag in &function.flags {
          flags |= *flag;
        }
        flags
      }
    };

    conn.create_scalar_function(
      function.name.as_str(),
      function.num_args as i32,
      flags,
      move |context| -> Result<rusqlite::types::Value, rusqlite::Error> {
        let args = (0..context.len())
          .map(|idx| -> Result<Value, rusqlite::Error> {
            return Ok(match context.get::<rusqlite::types::Value>(idx)? {
              rusqlite::types::Value::Null => Value::Null,
              rusqlite::types::Value::Integer(i) => Value::Integer(i),
              rusqlite::types::Value::Real(r) => Value::Real(r),
              rusqlite::types::Value::Text(s) => Value::Text(s),
              rusqlite::types::Value::Blob(b) => Value::Blob(b),
            });
          })
          .collect::<Result<Vec<_>, _>>()?;

        let value = rt
          .dispatch_scalar_function(function_name.clone(), args)
          .map_err(|err| {
            return rusqlite::Error::UserFunctionError(err.into());
          })?;

        return Ok(match value {
          Value::Null => rusqlite::types::Value::Null,
          Value::Integer(i) => rusqlite::types::Value::Integer(i),
          Value::Real(r) => rusqlite::types::Value::Real(r),
          Value::Text(s) => rusqlite::types::Value::Text(s),
          Value::Blob(b) => rusqlite::types::Value::Blob(b),
        });
      },
    )?;
  }

  return Ok(());
}

pub(crate) const ABI_MISMATCH_WARNING: &str = "\
    This may happen if the server and component are ABI incompatible. Make sure to run compatible \
    versions, i.e. update/rebuild the component to match the server binary or update your server \
    to run more up-to-date components.\n\
    The auth-UI can be updated with `$ trail components add trailbase/auth_ui` or downloaded from: \
    https://github.com/trailbaseio/trailbase/releases.";

#[cfg(test)]
mod tests {
  use super::*;
  use sync::exports::trailbase::component::init_endpoint::Arguments;

  const WASM_COMPONENT_PATH: &str = "../../client/testfixture/wasm/wasm_guest_testfixture.wasm";

  #[tokio::test]
  async fn test_init() {
    let runtime = SqliteFunctionRuntime::new(
      WASM_COMPONENT_PATH.into(),
      RuntimeOptions {
        ..Default::default()
      },
    )
    .unwrap();

    let (mut store, bindings) = runtime.new_bindings().unwrap();
    let api = bindings.trailbase_component_init_endpoint();

    let args = Arguments { version: None };

    api.call_init_http_handlers(&mut store, &args).unwrap();
  }
}
