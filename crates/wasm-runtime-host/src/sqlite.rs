use bytes::Bytes;
use http_body_util::{BodyExt, combinators::UnsyncBoxBody};
use rusqlite::Transaction;
use self_cell::{MutBorrow, self_cell};
use tokio::time::Duration;
use trailbase_sqlite::connection::ArcLockGuard;
use trailbase_sqlvalue::{DecodeError, SqlValue};
use trailbase_wasm_common::{SqliteRequest, SqliteResponse};
use wasmtime_wasi_http::bindings::http::types::ErrorCode;

self_cell!(
  pub(crate) struct OwnedTx {
    owner: MutBorrow<ArcLockGuard>,

    #[covariant]
    dependent: Transaction,
  }
);

pub(crate) async fn new_tx(conn: trailbase_sqlite::Connection) -> Result<OwnedTx, rusqlite::Error> {
  for _ in 0..200 {
    let Some(lock) = conn.try_write_arc_lock_for(Duration::from_micros(100)) else {
      tokio::time::sleep(Duration::from_micros(400)).await;
      continue;
    };

    return OwnedTx::try_new(MutBorrow::new(lock), |owner| {
      return owner.borrow_mut().transaction();
    });
  }

  return Err(rusqlite::Error::ToSqlConversionFailure(
    "Failed to acquire lock".into(),
  ));
}

async fn handle_sqlite_request_impl(
  conn: trailbase_sqlite::Connection,
  request: hyper::Request<wasmtime_wasi_http::body::HyperOutgoingBody>,
) -> Result<SqliteResponse, String> {
  return match request.uri().path() {
    "/execute" => {
      let sqlite_request = to_request(request).await?;

      let rows_affected = conn
        .execute(
          sqlite_request.query,
          sql_values_to_sqlite_params(sqlite_request.params).map_err(sqlite_err)?,
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
          sql_values_to_sqlite_params(sqlite_request.params).map_err(sqlite_err)?,
        )
        .await
        .map_err(sqlite_err)?;

      let json_rows = rows
        .iter()
        .map(convert_values)
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

pub(crate) fn sql_values_to_sqlite_params(
  values: Vec<SqlValue>,
) -> Result<Vec<trailbase_sqlite::Value>, DecodeError> {
  return values.into_iter().map(|p| p.try_into()).collect();
}

pub fn convert_values(row: &trailbase_sqlite::Row) -> Result<Vec<SqlValue>, String> {
  return (0..row.column_count())
    .map(|i| -> Result<SqlValue, String> {
      let value = row.get_value(i).ok_or_else(|| "not found".to_string())?;
      return Ok(value.into());
    })
    .collect();
}

#[inline]
pub fn bytes_to_body<E>(bytes: Bytes) -> UnsyncBoxBody<Bytes, E> {
  UnsyncBoxBody::new(http_body_util::Full::new(bytes).map_err(|_| unreachable!()))
}

#[inline]
pub fn sqlite_err<E: std::error::Error>(err: E) -> String {
  return err.to_string();
}

// #[inline]
// fn empty<E>() -> BoxBody<Bytes, E> {
//   BoxBody::new(http_body_util::Empty::new().map_err(|_| unreachable!()))
// }
