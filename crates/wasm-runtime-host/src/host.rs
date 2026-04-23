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
        "trailbase:database/sqlite.[constructor]transaction": async | trappable,
        "trailbase:database/sqlite.[drop]transaction": trappable,
        "trailbase:database/sqlite.[method]transaction.commit":async | trappable,
        "trailbase:database/sqlite.[method]transaction.rollback": async | trappable,
        "trailbase:database/sqlite.[method]transaction.query": async | trappable,
        "trailbase:database/sqlite.[method]transaction.execute": async | trappable,
        default: async,
    },
    with: {
        "trailbase:database/sqlite.transaction": self::TransactionImpl,
    },
    exports: {
        default: async | store,
    },
});

pub use self::trailbase::database::sqlite::{Transaction, TxError, Value};

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
  #[deprecated = "Used by deprecated `tx-*` free functions. Will be removed in favor of the `TransactionImpl` resource."]
  pub(crate) tx: Mutex<TransactionImpl>,

  // State shared across all runtime instances.
  pub(crate) shared: Arc<SharedState>,
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
  // async fn query(&mut self, query: String, params: Vec<Value>) -> Result<Vec<Vec<Value>>,
  // TxError> {   return Err(TxError::Other("not implemented".into()));
  // }

  async fn tx_begin(&mut self) -> Result<(), TxError> {
    let Some(conn) = self.shared.conn.clone() else {
      return Err(TxError::Other("missing conn".into()));
    };

    // Acquire shared lock first, before locking DB.
    #[allow(deprecated)]
    let mut lock = self.tx.lock().await;

    *lock = TransactionImpl::new(conn).await?;

    return Ok(());
  }

  async fn tx_commit(&mut self) -> Result<(), TxError> {
    #[allow(deprecated)]
    let mut lock = self.tx.lock().await;
    let tx: &mut TransactionImpl = &mut lock;
    return tx.commit().await;
  }

  async fn tx_rollback(&mut self) -> Result<(), TxError> {
    #[allow(deprecated)]
    let mut lock = self.tx.lock().await;
    let tx: &mut TransactionImpl = &mut lock;
    return tx.rollback().await;
  }

  async fn tx_execute(&mut self, query: String, params: Vec<Value>) -> Result<u64, TxError> {
    #[allow(deprecated)]
    return self.tx.lock().await.execute(query, params).await;
  }

  async fn tx_query(
    &mut self,
    query: String,
    params: Vec<Value>,
  ) -> Result<Vec<Vec<Value>>, TxError> {
    #[allow(deprecated)]
    return self.tx.lock().await.query(query, params).await;
  }
}

#[derive(Default)]
pub struct TransactionImpl {
  // NOTE: This is only an `Arc<Mutex<OwnedTx>>` to have a watcher task force-unlock DB if
  // necessary. W/o the task, this could just be an `OwnedTx`. The Mutex is an async Mutex to
  // prevent blocking the watcher task, e.g. if a transaction is blocked on a long-running
  // `tx.query`.
  tx: Arc<Mutex<Option<crate::sqlite::OwnedTx>>>,
}

impl TransactionImpl {
  async fn new(conn: trailbase_sqlite::Connection) -> Result<Self, TxError> {
    let db_lock =
      crate::sqlite::acquire_transaction_lock_with_timeout(conn, Duration::from_secs(2))
        .await
        .map_err(|err| TxError::Other(err.to_string()))?;

    let tx = Arc::new(Mutex::new(Some(db_lock)));

    {
      // Watcher task to unlock stuck transactions.
      let tx = Arc::downgrade(&tx);
      tokio::spawn(async move {
        const TIMEOUT: Duration = Duration::from_secs(60);
        tokio::time::sleep(TIMEOUT).await;
        if let Some(tx) = tx.upgrade() {
          // NOTE: Dropping the OwnedTx does all the cleanup of both issuing rollback and
          // releasing the DB lock.
          if tx.lock().await.take().is_some() {
            log::warn!("Pending WASM transaction lock found. Force-unlocked DB after {TIMEOUT:?}.");
          }
        }
      });
    }

    return Ok(Self { tx });
  }

  async fn commit(&mut self) -> Result<(), TxError> {
    let Some(tx) = self.tx.lock().await.take() else {
      return Err(TxError::Other("no pending tx".to_string()));
    };

    // NOTE: this is the same as `tx.commit()` just w/o consuming.
    if let Err(err) = tx.borrow_dependent().execute_batch("COMMIT") {
      return Err(TxError::Other(err.to_string()));
    }

    return Ok(());
  }

  async fn rollback(&mut self) -> Result<(), TxError> {
    let Some(tx) = self.tx.lock().await.take() else {
      return Ok(());
    };

    // NOTE: this is the same as `tx.rollback()` just w/o consuming.
    tx.borrow_dependent()
      .execute_batch("ROLLBACK")
      .map_err(|err| TxError::Other(err.to_string()))?;

    return Ok(());
  }

  async fn query(&self, query: String, params: Vec<Value>) -> Result<Vec<Vec<Value>>, TxError> {
    let lock = self.tx.lock().await;
    let Some(ref tx) = *lock else {
      return Err(TxError::Other("No open transaction".to_string()));
    };

    let mut stmt = tx
      .borrow_dependent()
      .prepare(&query)
      .map_err(|err| TxError::Other(err.to_string()))?;

    let params: Vec<_> = params.into_iter().map(to_sqlite_value).collect();
    params
      .bind(&mut stmt)
      .map_err(|err| TxError::Other(err.to_string()))?;

    let rows = trailbase_sqlite::sqlite::from_rows(stmt.raw_query())
      .map_err(|err| TxError::Other(err.to_string()))?;

    return Ok(
      rows
        .into_iter()
        .map(|trailbase_sqlite::Row(row, _col)| {
          return row.into_iter().map(from_sqlite_value).collect::<Vec<_>>();
        })
        .collect(),
    );
  }

  async fn execute(&self, query: String, params: Vec<Value>) -> Result<u64, TxError> {
    let lock = self.tx.lock().await;
    let Some(ref tx) = *lock else {
      return Err(TxError::Other("No open transaction".to_string()));
    };

    let mut stmt = tx
      .borrow_dependent()
      .prepare(&query)
      .map_err(|err| TxError::Other(err.to_string()))?;

    let params: Vec<_> = params.into_iter().map(to_sqlite_value).collect();
    params
      .bind(&mut stmt)
      .map_err(|err| TxError::Other(err.to_string()))?;

    return stmt
      .raw_execute()
      .map_err(|err| TxError::Other(err.to_string()))
      .map(|n| n as u64);
  }
}

impl self::trailbase::database::sqlite::HostTransaction for State {
  async fn new(&mut self) -> Result<Resource<Transaction>, wasmtime::Error> {
    let Some(conn) = self.shared.conn.clone() else {
      return Err(wasmtime::Error::msg("missing conn"));
    };

    return Ok(self.table().push(TransactionImpl::new(conn).await?)?);
  }

  async fn commit(
    &mut self,
    r: Resource<Transaction>,
  ) -> Result<Result<(), TxError>, wasmtime::Error> {
    let resource: &mut TransactionImpl = self.resource_table.get_mut(&r)?;
    return Ok(resource.commit().await);
  }

  async fn rollback(
    &mut self,
    r: Resource<Transaction>,
  ) -> Result<Result<(), TxError>, wasmtime::Error> {
    let resource: &mut TransactionImpl = self.resource_table.get_mut(&r)?;
    return Ok(resource.rollback().await);
  }

  async fn query(
    &mut self,
    r: Resource<Transaction>,
    query: String,
    params: Vec<Value>,
  ) -> Result<Result<Vec<Vec<Value>>, TxError>, wasmtime::Error> {
    let resource: &TransactionImpl = self.resource_table.get(&r)?;
    return Ok(resource.query(query, params).await);
  }

  async fn execute(
    &mut self,
    r: Resource<Transaction>,
    query: String,
    params: Vec<Value>,
  ) -> Result<Result<u64, TxError>, wasmtime::Error> {
    let resource: &TransactionImpl = self.resource_table.get(&r)?;
    return Ok(resource.execute(query, params).await);
  }

  fn drop(&mut self, r: Resource<Transaction>) -> Result<(), wasmtime::Error> {
    // NOTE: Dropping the OwnedTx does all the cleanup of both issuing rollback and
    // releasing the DB lock.
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
