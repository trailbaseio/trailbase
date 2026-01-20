use core::future::Future;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use trailbase_sqlite::{Params, Rows};
use trailbase_wasi_keyvalue::WasiKeyValueCtx;
use wasmtime::Result;
use wasmtime::component::{HasData, ResourceTable};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};
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
        "trailbase:database/sqlite.tx-begin": async,
    },
    exports: {
        default: async,
    },
});

use self::trailbase::database::sqlite::{TxError, Value};

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
  pub(crate) http: WasiHttpCtx,
  pub(crate) kv: WasiKeyValueCtx,

  pub(crate) shared: Arc<SharedState>,
  pub(crate) tx: Arc<Mutex<Option<crate::sqlite::OwnedTx>>>,
}

impl Drop for State {
  fn drop(&mut self) {
    #[cfg(debug_assertions)]
    if self.tx.lock().is_some() {
      log::warn!("pending transaction locking the DB");
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

impl WasiHttpView for State {
  fn ctx(&mut self) -> &mut WasiHttpCtx {
    return &mut self.http;
  }

  fn table(&mut self) -> &mut ResourceTable {
    return &mut self.resource_table;
  }

  /// Receives HTTP fetches from the guest.
  ///
  /// Based on `WasiView`' default implementation.
  fn send_request(
    &mut self,
    request: hyper::Request<wasmtime_wasi_http::body::HyperOutgoingBody>,
    config: wasmtime_wasi_http::types::OutgoingRequestConfig,
  ) -> wasmtime_wasi_http::HttpResult<wasmtime_wasi_http::types::HostFutureIncomingResponse> {
    // log::debug!(
    //   "send_request {:?} {}: {request:?}",
    //   request.uri().host(),
    //   request.uri().path()
    // );

    return match request.uri().host() {
      Some("__sqlite") => {
        let conn = self.shared.conn.clone().ok_or_else(|| {
          debug_assert!(false, "missing SQLite connection");
          wasmtime_wasi_http::bindings::http::types::ErrorCode::InternalError(Some(
            "missing SQLite connection".to_string(),
          ))
        })?;
        Ok(
          wasmtime_wasi_http::types::HostFutureIncomingResponse::pending(
            wasmtime_wasi::runtime::spawn(async move {
              Ok(crate::sqlite::handle_sqlite_request(conn, request).await)
            }),
          ),
        )
      }
      _ => {
        let handle = wasmtime_wasi::runtime::spawn(async move {
          Ok(wasmtime_wasi_http::types::default_send_request_handler(request, config).await)
        });
        Ok(wasmtime_wasi_http::types::HostFutureIncomingResponse::pending(handle))
      }
    };
  }
}
impl HasData for State {
  type Data<'a> = &'a mut State;
}

impl self::trailbase::database::sqlite::Host for State {
  fn tx_begin(&mut self) -> impl Future<Output = Result<(), TxError>> + Send {
    async fn begin(
      conn: trailbase_sqlite::Connection,
      tx: &Mutex<Option<crate::sqlite::OwnedTx>>,
    ) -> Result<(), TxError> {
      assert!(tx.lock().is_none());

      *tx.lock() = Some(
        crate::sqlite::new_tx(conn)
          .await
          .map_err(|err| TxError::Other(err.to_string()))?,
      );

      return Ok(());
    }

    let tx = self.tx.clone();
    return async move {
      let Some(conn) = self.shared.conn.clone() else {
        return Err(TxError::Other("missing conn".into()));
      };
      begin(conn, &tx).await
    };
  }

  fn tx_commit(&mut self) -> Result<(), TxError> {
    fn commit(tx: &Mutex<Option<crate::sqlite::OwnedTx>>) -> Result<(), TxError> {
      let Some(tx) = tx.lock().take() else {
        return Err(TxError::Other("no pending tx".to_string()));
      };

      // NOTE: this is the same as `tx.commit()` just w/o consuming.
      let lock = tx.borrow_dependent();
      lock
        .execute_batch("COMMIT")
        .map_err(|err| TxError::Other(err.to_string()))?;

      return Ok(());
    }

    return commit(&self.tx);
  }

  fn tx_rollback(&mut self) -> Result<(), TxError> {
    fn rollback(tx: &Mutex<Option<crate::sqlite::OwnedTx>>) -> Result<(), TxError> {
      let Some(tx) = tx.lock().take() else {
        return Err(TxError::Other("no pending tx".to_string()));
      };

      // NOTE: this is the same as `tx.rollback()` just w/o consuming.
      let lock = tx.borrow_dependent();
      lock
        .execute_batch("ROLLBACK")
        .map_err(|err| TxError::Other(err.to_string()))?;

      return Ok(());
    }

    return rollback(&self.tx);
  }

  fn tx_execute(&mut self, query: String, params: Vec<Value>) -> Result<u64, TxError> {
    fn execute(
      tx: &Mutex<Option<crate::sqlite::OwnedTx>>,
      query: String,
      params: Vec<Value>,
    ) -> Result<u64, TxError> {
      let params: Vec<_> = params.into_iter().map(to_sqlite_value).collect();

      let Some(ref tx) = *tx.lock() else {
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

    return execute(&self.tx, query, params);
  }

  fn tx_query(&mut self, query: String, params: Vec<Value>) -> Result<Vec<Vec<Value>>, TxError> {
    fn query_fn(
      tx: &Mutex<Option<crate::sqlite::OwnedTx>>,
      query: String,
      params: Vec<Value>,
    ) -> Result<Vec<Vec<Value>>, TxError> {
      let params: Vec<_> = params.into_iter().map(to_sqlite_value).collect();

      let Some(ref tx) = *tx.lock() else {
        return Err(TxError::Other("No open transaction".to_string()));
      };

      let lock = tx.borrow_dependent();
      let mut stmt = lock
        .prepare(&query)
        .map_err(|err| TxError::Other(err.to_string()))?;

      params
        .bind(&mut stmt)
        .map_err(|err| TxError::Other(err.to_string()))?;

      let rows =
        Rows::from_rows(stmt.raw_query()).map_err(|err| TxError::Other(err.to_string()))?;

      let values: Vec<_> = rows
        .into_iter()
        .map(|trailbase_sqlite::Row(row, _col)| {
          return row.into_iter().map(from_sqlite_value).collect::<Vec<_>>();
        })
        .collect();

      return Ok(values);
    }

    return query_fn(&self.tx, query, params);
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
