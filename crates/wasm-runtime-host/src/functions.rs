use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::{Result, Store};

use crate::Error;

#[derive(Clone)]
pub struct SqliteScalarFunction {
  pub name: String,
  pub num_args: u32,
  pub flags: Vec<rusqlite::functions::FunctionFlags>,
}

#[derive(Clone)]
pub struct SqliteFunctions {
  pub scalar_functions: Vec<SqliteScalarFunction>,
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
  pub async fn initialize_sqlite_functions(
    &self,
    args: crate::InitArgs,
  ) -> Result<SqliteFunctions, Error> {
    let api = self.state.bindings.trailbase_component_init_endpoint();

    let args = crate::host::exports::trailbase::component::init_endpoint::Arguments {
      version: args.version,
    };

    return Ok(SqliteFunctions {
      scalar_functions: api
        .call_init_sqlite_functions(&mut *self.state.store.lock().await, &args)
        .await?
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

    return api
      .call_dispatch_scalar_function(&mut *self.state.store.lock().await, &args)
      .await?
      .map_err(|err| {
        return Error::Other(err.to_string());
      });
  }
}

pub fn setup_connection(
  conn: &rusqlite::Connection,
  store: SqliteStore,
  functions: &SqliteFunctions,
) -> Result<(), rusqlite::Error> {
  use crate::host::exports::trailbase::component::sqlite_function_endpoint::Value;

  for function in &functions.scalar_functions {
    let store = store.clone();
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

    // Registers the WASM function with the SQLite connection.
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

  return Ok(());
}
