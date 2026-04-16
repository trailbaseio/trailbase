use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;
use trailbase_sqlite::Params;
use trailbase_wasi_keyvalue::WasiKeyValueCtx;
use wasmtime::Result;
use wasmtime::component::{HasData, Resource, ResourceTable};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::WasiHttpCtx;
use wasmtime_wasi_http::p2::{WasiHttpHooks, WasiHttpView};
use wasmtime_wasi_io::IoView;

use crate::sqlite::acquire_transaction_lock_with_timeout;

// Documentation: https://docs.wasmtime.dev/api/wasmtime/component/macro.bindgen.html
wasmtime::component::bindgen!({
    world: "trailbase:component/interfaces",
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
        "wit/trailbase/database",
        "wit/trailbase/component",
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
        "trailbase:database/sqlite.tx-begin": async,
        "trailbase:database/sqlite.tx-commit": async,
        "trailbase:database/sqlite.tx-rollback": async,
        "trailbase:database/sqlite.tx-execute": async,
        "trailbase:database/sqlite.tx-query": async,
        "trailbase:database/sqlite.[constructor]transaction": async | trappable,
        default: async,
    },
    with: {
        "trailbase:database/sqlite.transaction": self::MyTransaction,
    },
    exports: {
        default: async | store,
    },
});

pub use self::trailbase::database::sqlite::{TxError, Value};

/// NOTE: This is needed due to State needing to be Send.
unsafe impl Send for crate::sqlite::OwnedTx {}

/// Shared state, which can be shared across multiple runtime instances.
pub struct SharedState {
  pub conn: Option<trailbase_sqlite::Connection>,
  pub kv_store: trailbase_wasi_keyvalue::Store,
  pub fs_root_path: Option<PathBuf>,
}

/// State for one runtime instance.
pub struct State {
  pub(crate) resource_table: ResourceTable,
  pub(crate) wasi_ctx: WasiCtx,
  pub(crate) http_ctx: WasiHttpCtx,
  pub(crate) hooks: Hooks,
  pub(crate) kv: WasiKeyValueCtx,

  // A mutex of a DB lock.
  pub(crate) tx: Mutex<Option<crate::sqlite::OwnedTx>>,

  // State shared across all runtime instances.
  pub(crate) shared: Arc<SharedState>,
}

impl Drop for State {
  fn drop(&mut self) {
    #[cfg(debug_assertions)]
    if self.tx.get_mut().is_some() {
      log::warn!(
        "pending transaction found during State destruction. Transactions should be committed, rolled back or dropped to unlock the DB."
      );
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

pub(crate) struct Hooks {
  pub shared: Arc<SharedState>,
}

impl WasiHttpHooks for Hooks {
  fn send_request(
    &mut self,
    request: hyper::Request<wasmtime_wasi_http::p2::body::HyperOutgoingBody>,
    config: wasmtime_wasi_http::p2::types::OutgoingRequestConfig,
  ) -> wasmtime_wasi_http::p2::HttpResult<wasmtime_wasi_http::p2::types::HostFutureIncomingResponse>
  {
    // log::debug!(
    //   "send_request {:?} {}: {request:?}",
    //   request.uri().host(),
    //   request.uri().path()
    // );

    return match request.uri().host() {
      Some("__sqlite") => {
        let conn = self.shared.conn.clone().ok_or_else(|| {
          debug_assert!(false, "missing SQLite connection");
          wasmtime_wasi_http::p2::bindings::http::types::ErrorCode::InternalError(Some(
            "missing SQLite connection".to_string(),
          ))
        })?;
        Ok(
          wasmtime_wasi_http::p2::types::HostFutureIncomingResponse::pending(
            wasmtime_wasi::runtime::spawn(async move {
              Ok(crate::sqlite::handle_sqlite_request(conn, request).await)
            }),
          ),
        )
      }
      _ => Ok(wasmtime_wasi_http::p2::default_send_request(
        request, config,
      )),
    };
  }
}

impl WasiHttpView for State {
  fn http(&mut self) -> wasmtime_wasi_http::p2::WasiHttpCtxView<'_> {
    wasmtime_wasi_http::p2::WasiHttpCtxView {
      ctx: &mut self.http_ctx,
      table: &mut self.resource_table,
      hooks: &mut self.hooks,
    }
  }
}

impl HasData for State {
  type Data<'a> = &'a mut State;
}

impl self::trailbase::database::sqlite::Host for State {
  // async fn execute(&mut self, query: String, params: Vec<Value>) -> Result<u64, TxError> {
  //   return Err(TxError::Other("not implemented".into()));
  // }
  // async fn query(&mut self, query: String, params: Vec<Value>) -> Result<Vec<Vec<Value>>, TxError> {
  //   return Err(TxError::Other("not implemented".into()));
  // }

  async fn tx_begin(&mut self) -> Result<(), TxError> {
    let Some(conn) = self.shared.conn.clone() else {
      return Err(TxError::Other("missing conn".into()));
    };

    let mut lock = self.tx.lock().await;
    assert!(lock.is_none());

    // TODO: Spawn a watcher task that unlocks the DB after a certain timeout.
    *lock = Some(
      acquire_transaction_lock_with_timeout(conn, Duration::from_millis(1000))
        .await
        .map_err(|err| TxError::Other(err.to_string()))?,
    );

    return Ok(());
  }

  async fn tx_commit(&mut self) -> Result<(), TxError> {
    let Some(tx) = self.tx.lock().await.take() else {
      return Err(TxError::Other("no pending tx".to_string()));
    };

    // NOTE: this is the same as `tx.commit()` just w/o consuming.
    let lock = tx.borrow_dependent();
    lock
      .execute_batch("COMMIT")
      .map_err(|err| TxError::Other(err.to_string()))?;

    return Ok(());
  }

  async fn tx_rollback(&mut self) -> Result<(), TxError> {
    let Some(tx) = self.tx.lock().await.take() else {
      return Err(TxError::Other("no pending tx".to_string()));
    };

    // NOTE: this is the same as `tx.rollback()` just w/o consuming.
    let lock = tx.borrow_dependent();
    lock
      .execute_batch("ROLLBACK")
      .map_err(|err| TxError::Other(err.to_string()))?;

    return Ok(());
  }

  async fn tx_execute(&mut self, query: String, params: Vec<Value>) -> Result<u64, TxError> {
    let params: Vec<_> = params.into_iter().map(to_sqlite_value).collect();

    let Some(ref tx) = *self.tx.lock().await else {
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

  async fn tx_query(
    &mut self,
    query: String,
    params: Vec<Value>,
  ) -> Result<Vec<Vec<Value>>, TxError> {
    let params: Vec<_> = params.into_iter().map(to_sqlite_value).collect();

    let Some(ref tx) = *self.tx.lock().await else {
      return Err(TxError::Other("No open transaction".to_string()));
    };

    let lock = tx.borrow_dependent();
    let mut stmt = lock
      .prepare(&query)
      .map_err(|err| TxError::Other(err.to_string()))?;

    params
      .bind(&mut stmt)
      .map_err(|err| TxError::Other(err.to_string()))?;

    let rows = trailbase_sqlite::sqlite::from_rows(stmt.raw_query())
      .map_err(|err| TxError::Other(err.to_string()))?;

    let values: Vec<_> = rows
      .into_iter()
      .map(|trailbase_sqlite::Row(row, _col)| {
        return row.into_iter().map(from_sqlite_value).collect::<Vec<_>>();
      })
      .collect();

    return Ok(values);
  }
}

type Transaction = self::trailbase::database::sqlite::Transaction;

pub struct MyTransaction {
  pub foo: i64,
}

impl self::trailbase::database::sqlite::HostTransaction for State {
  async fn new(&mut self) -> Result<Resource<Transaction>, wasmtime::Error> {
    return Ok(self.table().push(MyTransaction { foo: 5 })?);
  }

  async fn begin(&mut self, _r: Resource<Transaction>) -> Result<(), TxError> {
    return Err(TxError::Other("not implemented".into()));
  }

  async fn commit(&mut self, _r: Resource<Transaction>) -> Result<(), TxError> {
    return Err(TxError::Other("not implemented".into()));
  }

  async fn rollback(&mut self, _r: Resource<Transaction>) -> Result<(), TxError> {
    return Err(TxError::Other("not implemented".into()));
  }

  async fn query(
    &mut self,
    _r: Resource<Transaction>,
    _query: String,
    _params: Vec<Value>,
  ) -> Result<Vec<Vec<Value>>, TxError> {
    return Err(TxError::Other("not implemented".into()));
  }

  async fn execute(
    &mut self,
    _r: Resource<Transaction>,
    _query: String,
    _params: Vec<Value>,
  ) -> Result<u64, TxError> {
    return Err(TxError::Other("not implemented".into()));
  }

  async fn drop(&mut self, r: Resource<Transaction>) -> Result<(), wasmtime::Error> {
    let x: &MyTransaction = self.resource_table.get(&r)?;
    println!("value: {}", x.foo);
    self.resource_table.delete(r)?;
    return Ok(());
  }
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
