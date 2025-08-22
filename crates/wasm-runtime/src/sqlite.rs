use bytes::Bytes;
use http_body_util::{BodyExt, combinators::BoxBody};
use rusqlite::Transaction;
use self_cell::{MutBorrow, self_cell};
use tokio::time::Duration;
use trailbase_schema::json::{JsonError, rich_json_to_value, value_to_rich_json};
use trailbase_sqlite::connection::ArcLockGuard;
use trailbase_wasm_common::{SqliteRequest, SqliteResponse};
use wasmtime_wasi_http::bindings::http::types::ErrorCode;

// self_cell!(
//   pub(crate) struct OwnedLock {
//     owner: trailbase_sqlite::Connection,
//
//     #[covariant]
//     dependent: LockGuard,
//   }
// );
//
// self_cell!(
//   pub(crate) struct OwnedTransaction {
//     owner: MutBorrow<OwnedLock>,
//
//     #[covariant]
//     dependent: Transaction,
//   }
// );
//
// pub(crate) async fn new_transaction(
//   conn: trailbase_sqlite::Connection,
// ) -> Result<OwnedTransaction, rusqlite::Error> {
//   loop {
//     let Ok(lock) = OwnedLock::try_new(conn.clone(), |owner| {
//       owner
//         .try_write_lock_for(Duration::from_micros(50))
//         .ok_or("timeout")
//     }) else {
//       tokio::time::sleep(Duration::from_micros(150)).await;
//       continue;
//     };
//
//     return OwnedTransaction::try_new(MutBorrow::new(lock), |owner| {
//       owner
//         .borrow_mut()
//         .with_dependent_mut(|_owner, depdendent| depdendent.transaction())
//     });
//   }
// }

self_cell!(
  pub(crate) struct OwnedTx {
    owner: MutBorrow<ArcLockGuard>,

    #[covariant]
    dependent: Transaction,
  }
);

// unsafe impl Sync for OwnedTx {}

pub(crate) async fn new_tx(conn: trailbase_sqlite::Connection) -> Result<OwnedTx, rusqlite::Error> {
  loop {
    let Some(lock) = conn.try_write_arc_lock_for(Duration::from_micros(50)) else {
      tokio::time::sleep(Duration::from_micros(150)).await;
      continue;
    };

    return OwnedTx::try_new(MutBorrow::new(lock), |owner| {
      return Ok(owner.borrow_mut().transaction()?);
    });
  }
}

// std::thread_local! {
//     // TODO: Could be a RefCell instead of a Mutex.
//     pub(crate) static CURRENT_TX: Mutex<Option<OwnedTx>> = const { Mutex::new(None) } ;
// }

async fn handle_sqlite_request_impl(
  conn: trailbase_sqlite::Connection,
  request: hyper::Request<wasmtime_wasi_http::body::HyperOutgoingBody>,
) -> Result<SqliteResponse, String> {
  return match request.uri().path() {
    // "/tx_begin" => {
    //   let new_tx = new_tx(conn).await.map_err(sqlite_err)?;
    //
    //   CURRENT_TX.with(|tx: &Mutex<_>| {
    //     *tx.lock() = Some(new_tx);
    //   });
    //
    //   Ok(SqliteResponse::TxBegin)
    // }
    // "/tx_commit" => {
    //   let tx = CURRENT_TX.with(|tx: &Mutex<_>| {
    //     return tx.lock().take();
    //   });
    //   if let Some(tx) = tx {
    //     // NOTE: this is the same as `tx.commit()` just w/o consuming.
    //     let lock = tx.borrow_dependent();
    //     lock.execute_batch("COMMIT").map_err(sqlite_err)?;
    //   }
    //
    //   Ok(SqliteResponse::TxCommit)
    // }
    // "/tx_execute" => {
    //   let sqlite_request = to_request(request).await?;
    //
    //   let params = json_values_to_sqlite_params(sqlite_request.params).map_err(sqlite_err)?;
    //
    //   let rows_affected = CURRENT_TX.with(move |tx: &Mutex<_>| -> Result<usize, String> {
    //     let Some(ref tx) = *tx.lock() else {
    //       return Err("No open transaction".to_string());
    //     };
    //     let lock = tx.borrow_dependent();
    //
    //     let mut stmt = lock.prepare(&sqlite_request.query).map_err(sqlite_err)?;
    //
    //     params.bind(&mut stmt).map_err(sqlite_err)?;
    //
    //     return stmt.raw_execute().map_err(sqlite_err);
    //   })?;
    //
    //   Ok(SqliteResponse::Execute { rows_affected })
    // }
    // "/tx_query " => {
    //   let sqlite_request = to_request(request).await?;
    //
    //   let params = json_values_to_sqlite_params(sqlite_request.params).map_err(sqlite_err)?;
    //
    //   let rows = CURRENT_TX.with(move |tx: &Mutex<_>| -> Result<Rows, String> {
    //     let Some(ref tx) = *tx.lock() else {
    //       return Err("No open transaction".to_string());
    //     };
    //     let lock = tx.borrow_dependent();
    //
    //     let mut stmt = lock.prepare(&sqlite_request.query).map_err(sqlite_err)?;
    //
    //     params.bind(&mut stmt).map_err(sqlite_err)?;
    //
    //     return Rows::from_rows(stmt.raw_query()).map_err(sqlite_err);
    //   })?;
    //
    //   let json_rows = rows
    //     .iter()
    //     .map(|row| -> Result<Vec<serde_json::Value>, String> {
    //       return row_to_rich_json_array(row).map_err(sqlite_err);
    //     })
    //     .collect::<Result<Vec<_>, _>>()?;
    //
    //   Ok(SqliteResponse::Query { rows: json_rows })
    // }
    // "/tx_rollback " => {
    //   let tx = CURRENT_TX.with(|tx: &Mutex<_>| {
    //     return tx.lock().take();
    //   });
    //   if let Some(tx) = tx {
    //     // NOTE: this is the same as `tx.rollback()` just w/o consuming.
    //     let lock = tx.borrow_dependent();
    //     lock.execute_batch("ROLLBACK").map_err(sqlite_err)?;
    //   }
    //
    //   Ok(SqliteResponse::TxRollback)
    // }
    "/execute" => {
      let sqlite_request = to_request(request).await?;

      let rows_affected = conn
        .execute(
          sqlite_request.query,
          json_values_to_sqlite_params(sqlite_request.params).map_err(sqlite_err)?,
        )
        .await
        .map_err(sqlite_err)?;

      Ok(SqliteResponse::Execute { rows_affected })
    }
    "/query" => {
      let sqlite_request = to_request(request).await?;

      let rows = conn
        .write_query_rows(
          sqlite_request.query,
          json_values_to_sqlite_params(sqlite_request.params).map_err(sqlite_err)?,
        )
        .await
        .map_err(sqlite_err)?;

      let json_rows = rows
        .iter()
        .map(|row| -> Result<Vec<serde_json::Value>, String> {
          return row_to_rich_json_array(row).map_err(sqlite_err);
        })
        .collect::<Result<Vec<_>, _>>()?;

      Ok(SqliteResponse::Query { rows: json_rows })
    }
    _ => Err("Not found".to_string()),
  };
}

pub(crate) async fn handle_sqlite_request(
  conn: trailbase_sqlite::Connection,
  request: hyper::Request<wasmtime_wasi_http::body::HyperOutgoingBody>,
) -> Result<wasmtime_wasi_http::types::IncomingResponse, ErrorCode> {
  return match handle_sqlite_request_impl(conn, request).await {
    Ok(response) => to_response(response),
    Err(err) => to_response(SqliteResponse::Error(err)),
  };
}

async fn to_request(
  request: hyper::Request<wasmtime_wasi_http::body::HyperOutgoingBody>,
) -> Result<SqliteRequest, String> {
  let (_parts, body) = request.into_parts();
  let bytes: Bytes = body.collect().await.map_err(sqlite_err)?.to_bytes();
  return serde_json::from_slice(&bytes).map_err(sqlite_err);
}

fn to_response(
  response: SqliteResponse,
) -> Result<wasmtime_wasi_http::types::IncomingResponse, ErrorCode> {
  let body =
    serde_json::to_vec(&response).map_err(|err| ErrorCode::InternalError(Some(err.to_string())))?;

  let resp = http::Response::builder()
    .status(200)
    .body(bytes_to_body(Bytes::from_owner(body)))
    .map_err(|err| ErrorCode::InternalError(Some(err.to_string())))?;

  return Ok(wasmtime_wasi_http::types::IncomingResponse {
    resp,
    worker: None,
    between_bytes_timeout: std::time::Duration::ZERO,
  });
}

pub(crate) fn json_values_to_sqlite_params(
  values: Vec<serde_json::Value>,
) -> Result<Vec<trailbase_sqlite::Value>, JsonError> {
  return values.into_iter().map(rich_json_to_value).collect();
}

pub fn row_to_rich_json_array(
  row: &trailbase_sqlite::Row,
) -> Result<Vec<serde_json::Value>, JsonError> {
  return (0..row.column_count())
    .map(|i| -> Result<serde_json::Value, JsonError> {
      let value = row.get_value(i).ok_or(JsonError::ValueNotFound)?;
      return value_to_rich_json(value);
    })
    .collect();
}

#[inline]
pub fn bytes_to_body<E>(bytes: Bytes) -> BoxBody<Bytes, E> {
  BoxBody::new(http_body_util::Full::new(bytes).map_err(|_| unreachable!()))
}

#[inline]
pub fn sqlite_err<E: std::error::Error>(err: E) -> String {
  return err.to_string();
}

// #[inline]
// fn empty<E>() -> BoxBody<Bytes, E> {
//   BoxBody::new(http_body_util::Empty::new().map_err(|_| unreachable!()))
// }
