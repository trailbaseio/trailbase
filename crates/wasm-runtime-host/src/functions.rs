use rusqlite::vtab::{
  Context, CreateVTab, Filters, IndexInfo, VTab, VTabConnection, VTabCursor, eponymous_only_module,
  read_only_module,
};
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::{Result, Store};

use crate::Error;

#[derive(Clone)]
pub struct SqliteScalarFunction {
  pub name: String,
  pub num_args: u32,
  pub flags: rusqlite::functions::FunctionFlags,
}

#[derive(Clone)]
pub struct SqliteModule {
  pub name: String,
}

#[derive(Clone)]
pub struct SqliteExtensions {
  pub scalar_functions: Vec<SqliteScalarFunction>,
  pub modules: Vec<SqliteModule>,
}

struct SqliteStoreInternal {
  store: Mutex<Store<crate::host::State>>,
  bindings: crate::host::Interfaces,
}

#[derive(Clone)]
pub struct SqliteStore {
  state: Arc<SqliteStoreInternal>,
}

impl SqliteStore {
  pub async fn new(runtime: &crate::Runtime) -> Result<Self, Error> {
    let (store, bindings) = runtime.new_bindings().await?;
    return Ok(Self {
      state: Arc::new(SqliteStoreInternal {
        store: Mutex::new(store),
        bindings,
      }),
    });
  }

  // Call WASM components `init` implementation.
  pub async fn initialize_sqlite_extensions(
    &self,
    args: crate::InitArgs,
  ) -> Result<SqliteExtensions, Error> {
    use crate::host::exports::trailbase::component::init_endpoint::{
      Arguments, SqliteExtensions as WitSqliteExtensions,
    };
    let api = self.state.bindings.trailbase_component_init_endpoint();

    let args = Arguments {
      version: args.version,
    };

    let mut store = self.state.store.lock().await;
    let WitSqliteExtensions {
      modules,
      scalar_functions,
    } = store
      .run_concurrent(async |accessor| -> Result<_, Error> {
        let (extensions, task_exit) = api.call_init_sqlite_extensions(accessor, args).await?;
        task_exit.block(accessor).await;
        return Ok(extensions);
      })
      .await??;

    return Ok(SqliteExtensions {
      scalar_functions: scalar_functions
        .into_iter()
        .map(|f| {
          return SqliteScalarFunction {
            name: f.name,
            num_args: f.num_args,
            flags: rusqlite::functions::FunctionFlags::from_bits_truncate(
              f.function_flags.as_array()[0] as i32,
            ),
          };
        })
        .collect(),
      modules: modules
        .into_iter()
        .map(|m| return SqliteModule { name: m.name })
        .collect(),
    });
  }

  pub async fn dispatch_scalar_function(
    &self,
    function_name: String,
    args: Vec<crate::host::exports::trailbase::component::sqlite_function_endpoint::Value>,
  ) -> Result<crate::host::exports::trailbase::component::sqlite_function_endpoint::Value, Error>
  {
    use crate::host::exports::trailbase::component::sqlite_function_endpoint::Arguments;

    let api = self
      .state
      .bindings
      .trailbase_component_sqlite_function_endpoint();

    let args = Arguments {
      function_name,
      arguments: args,
    };

    let mut store = self.state.store.lock().await;
    let result = store
      .run_concurrent(async |accessor| -> Result<_, Error> {
        let (result, task_exit) = api.call_dispatch_scalar_function(accessor, args).await?;
        task_exit.block(accessor).await;
        return Ok(result);
      })
      .await??;

    return result.map_err(|err| {
      return Error::Other(err.to_string());
    });
  }
}

pub fn setup_connection(
  conn: &rusqlite::Connection,
  store: SqliteStore,
  extensions: SqliteExtensions,
) -> Result<(), rusqlite::Error> {
  use crate::host::exports::trailbase::component::sqlite_function_endpoint::Value;

  for function in &extensions.scalar_functions {
    let store = store.clone();
    let function_name = function.name.clone();

    // Registers the WASM function with the SQLite connection.
    conn.create_scalar_function(
      function.name.as_str(),
      function.num_args as i32,
      function.flags,
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

        // This is where the actual dispatch happens in a stateless manner, i.e. subsequent
        // executions don't share state.
        let tokio = tokio::runtime::Builder::new_current_thread()
          .enable_time()
          .build()
          .expect("running on a 'raw' thread of trailbase-sqlite");

        let value = tokio
          .block_on(store.dispatch_scalar_function(function_name.clone(), args))
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

  for module in extensions.modules {
    // TODO: call runtime to create module resource and then pass resource to Module.

    conn
      .create_module(
        module.name.as_str(),
        // &eponymous_only_module::<WasmVTab>(),
        &read_only_module::<WasmVTab>(),
        None,
      )
      .expect("FIXME");
  }

  return Ok(());
}

// TODO: Should maybe implement UpdateVTab instead of just VTab. Should hold a WASM
// resource.
#[repr(C)]
struct WasmVTab {
  /// Base class. Must be first
  base: rusqlite::vtab::sqlite3_vtab,
}

#[allow(unsafe_code)]
unsafe impl<'vtab> VTab<'vtab> for WasmVTab {
  type Aux = ();
  type Cursor = WasmVTabCursor<'vtab>;

  fn connect(
    _: &mut VTabConnection,
    _aux: Option<&()>,
    _args: &[&[u8]],
  ) -> rusqlite::Result<(String, Self)> {
    let vtab = Self {
      base: rusqlite::ffi::sqlite3_vtab::default(),
    };
    return Ok(("CREATE TABLE x(value)".to_owned(), vtab));
  }

  fn best_index(&self, info: &mut IndexInfo) -> rusqlite::Result<()> {
    info.set_estimated_cost(1.);
    return Ok(());
  }

  fn open(&mut self) -> rusqlite::Result<WasmVTabCursor<'_>> {
    Ok(WasmVTabCursor::default())
  }
}

impl<'vtab> CreateVTab<'vtab> for WasmVTab {
  const KIND: rusqlite::vtab::VTabKind = rusqlite::vtab::VTabKind::Default;
}

#[derive(Default)]
#[repr(C)]
struct WasmVTabCursor<'vtab> {
  /// Base class. Must be first
  base: rusqlite::ffi::sqlite3_vtab_cursor,
  /// The rowid
  row_id: i64,
  phantom: PhantomData<&'vtab WasmVTab>,
}

unsafe impl VTabCursor for WasmVTabCursor<'_> {
  fn filter(
    &mut self,
    _idx_num: std::ffi::c_int,
    _idx_str: Option<&str>,
    _args: &Filters<'_>,
  ) -> rusqlite::Result<()> {
    self.row_id = 1;
    return Ok(());
  }

  fn next(&mut self) -> rusqlite::Result<()> {
    self.row_id += 1;
    return Ok(());
  }

  fn eof(&self) -> bool {
    return self.row_id > 1;
  }

  fn column(&self, ctx: &mut Context, i: std::ffi::c_int) -> rusqlite::Result<()> {
    return ctx.set_result(&self.row_id);
  }

  fn rowid(&self) -> rusqlite::Result<i64> {
    return Ok(self.row_id);
  }
}
