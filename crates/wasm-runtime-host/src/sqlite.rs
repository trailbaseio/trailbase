use bytes::Bytes;
use http::Uri;
use http_body_util::{BodyExt, combinators::UnsyncBoxBody};
use rusqlite::Transaction;
use self_cell::{MutBorrow, self_cell};
use sqlite3_parser::ast::Stmt;
use tokio::time::Duration;
use trailbase_schema::parse::parse_into_statement;
use trailbase_schema::sqlite::unquote_expr;
use trailbase_sqlite::{Rows, connection::ArcLockGuard};
use trailbase_sqlvalue::{DecodeError, SqlValue};
use trailbase_wasm_common::{SqliteRequest, SqliteResponse};
use wasmtime_wasi_http::p2::bindings::http::types::ErrorCode;

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

async fn handle_sqlite_execute(
  conn: trailbase_sqlite::Connection,
  request: SqliteRequest,
) -> Result<SqliteResponse, String> {
  // NOTE: We're doing redundant work here: first we parse and then SQLite parses again. We do
  // this to more intelligently schedule requests based on whether they're reads or writes w/o
  // requiring users to use two separate entry points and possibly making a mistake. Ultimately,
  // we believe it's worth it to allow cheap reads.
  let rows_affected = match parse_into_statement(&request.query) {
    // NOTE: We need to handle connection mutations (attach, detach) specially, so that they
    // apply to all internal read and write connections.
    Ok(Some(Stmt::Attach { expr, db_name, .. })) => {
      conn
        .attach(&unquote_expr(&expr), &unquote_expr(&db_name))
        .map_err(sqlite_err)?;
      0
    }
    Ok(Some(Stmt::Detach(name))) => {
      conn.detach(&unquote_expr(&name)).map_err(sqlite_err)?;
      0
    }
    _ => {
      match conn
        .execute(
          request.query,
          sql_values_to_sqlite_params(request.params).map_err(sqlite_err)?,
        )
        .await
      {
        Ok(rows_affected) => rows_affected,
        Err(err) => {
          return Ok(SqliteResponse::Error(err.to_string()));
        }
      }
    }
  };

  Ok(SqliteResponse::Execute { rows_affected })
}

async fn handle_sqlite_query(
  conn: trailbase_sqlite::Connection,
  request: SqliteRequest,
) -> Result<SqliteResponse, String> {
  // Handles write queries.
  async fn write(
    conn: trailbase_sqlite::Connection,
    request: SqliteRequest,
  ) -> Result<Rows, String> {
    return conn
      .write_query_rows(
        request.query,
        sql_values_to_sqlite_params(request.params).map_err(sqlite_err)?,
      )
      .await
      .map_err(sqlite_err);
  }

  // Handles read queries.
  async fn read(
    conn: trailbase_sqlite::Connection,
    request: SqliteRequest,
  ) -> Result<Rows, String> {
    return conn
      .read_query_rows(
        request.query,
        sql_values_to_sqlite_params(request.params).map_err(sqlite_err)?,
      )
      .await
      .map_err(sqlite_err);
  }

  // NOTE: We're doing redundant work here: first we parse and then SQLite parses again. We do
  // this to more intelligently schedule requests based on whether they're reads or writes w/o
  // requiring users to use two separate entry points and possibly making a mistake. Ultimately,
  // we believe it's worth it to allow cheap reads.
  let rows = match parse_into_statement(&request.query) {
    // NOTE: We need to handle connection mutations (attach, detach) specially, so that they
    // apply to all internal read and write connections.
    Ok(Some(Stmt::Attach { expr, db_name, .. })) => {
      conn
        .attach(&unquote_expr(&expr), &unquote_expr(&db_name))
        .map_err(sqlite_err)?;
      Rows::empty()
    }
    Ok(Some(Stmt::Detach(name))) => {
      conn.detach(&unquote_expr(&name)).map_err(sqlite_err)?;
      Rows::empty()
    }
    Ok(Some(Stmt::Select(_))) => match read(conn, request).await {
      Ok(rows) => rows,
      Err(err) => {
        return Ok(SqliteResponse::Error(err));
      }
    },
    _ => match write(conn, request).await {
      Ok(rows) => rows,
      Err(err) => {
        return Ok(SqliteResponse::Error(err));
      }
    },
  };

  let json_rows = rows
    .iter()
    .map(convert_values)
    .collect::<Result<Vec<_>, _>>()?;

  return Ok(SqliteResponse::Query { rows: json_rows });
}

pub(crate) async fn handle_sqlite_request(
  conn: trailbase_sqlite::Connection,
  request: hyper::Request<wasmtime_wasi_http::p2::body::HyperOutgoingBody>,
) -> Result<wasmtime_wasi_http::p2::types::IncomingResponse, ErrorCode> {
  let (uri, sqlite_request) = match to_request(request).await {
    Ok(request) => request,
    Err(err) => {
      return to_response(SqliteResponse::Error(err));
    }
  };

  let response = match uri.path() {
    "/execute" => handle_sqlite_execute(conn, sqlite_request).await,
    "/query" => handle_sqlite_query(conn, sqlite_request).await,
    _ => {
      // NOTE: Should not happen and doesn't need to be handled by the client as
      // SqliteResponse::Error.
      return Err(ErrorCode::InternalError(Some(format!(
        "Invalid path: {uri}"
      ))));
    }
  };

  return match response {
    Ok(response) => to_response(response),
    Err(err) => to_response(SqliteResponse::Error(err)),
  };
}

async fn to_request(
  request: hyper::Request<wasmtime_wasi_http::p2::body::HyperOutgoingBody>,
) -> Result<(Uri, SqliteRequest), String> {
  let (parts, body) = request.into_parts();
  let bytes: Bytes = body.collect().await.map_err(sqlite_err)?.to_bytes();
  return Ok((
    parts.uri,
    serde_json::from_slice(&bytes).map_err(sqlite_err)?,
  ));
}

fn to_response(
  response: SqliteResponse,
) -> Result<wasmtime_wasi_http::p2::types::IncomingResponse, ErrorCode> {
  let body =
    serde_json::to_vec(&response).map_err(|err| ErrorCode::InternalError(Some(err.to_string())))?;

  let resp = http::Response::builder()
    .status(200)
    .body(bytes_to_body(Bytes::from_owner(body)))
    .map_err(|err| ErrorCode::InternalError(Some(err.to_string())))?;

  return Ok(wasmtime_wasi_http::p2::types::IncomingResponse {
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

#[cfg(test)]
mod tests {
  use super::*;
  use trailbase_sqlite::Connection;

  #[tokio::test]
  async fn handle_sqlite_execute_test() {
    let conn = Connection::open_in_memory().unwrap();

    let _ = handle_sqlite_execute(
      conn.clone(),
      SqliteRequest {
        query: "CREATE TABLE test (id INTEGER PRIMARY KEY)".to_string(),
        params: vec![],
      },
    )
    .await
    .unwrap();

    let response = handle_sqlite_execute(
      conn.clone(),
      SqliteRequest {
        query: "INSERT INTO test (id) VALUES (?1), (?2)".to_string(),
        params: vec![SqlValue::Integer(2), SqlValue::Integer(3)],
      },
    )
    .await
    .unwrap();

    let SqliteResponse::Execute { rows_affected } = response else {
      panic!("expected execute, got: {response:?}");
    };

    assert_eq!(2, rows_affected);

    // Attach
    let _ = handle_sqlite_execute(
      conn.clone(),
      SqliteRequest {
        query: "ATTACH DATABASE ':memory:' AS foo".to_string(),
        params: vec![],
      },
    )
    .await
    .unwrap();

    // Detach
    let _ = handle_sqlite_execute(
      conn.clone(),
      SqliteRequest {
        query: "DETACH DATABASE foo".to_string(),
        params: vec![],
      },
    )
    .await
    .unwrap();

    // Error
    let response = handle_sqlite_execute(
      conn.clone(),
      SqliteRequest {
        query: "NOT A VALID QUERY :)".to_string(),
        params: vec![],
      },
    )
    .await
    .unwrap();

    let SqliteResponse::Error(err) = response else {
      panic!("expected error, got: {response:?}");
    };

    assert!(
      err.contains("near \"NOT\": syntax error in NOT A VALID QUERY :) at offset 0"),
      "Got: {err:?}"
    );
  }

  #[tokio::test]
  async fn handle_sqlite_query_test() {
    let conn = Connection::open_in_memory().unwrap();

    // Write.
    let _ = handle_sqlite_query(
      conn.clone(),
      SqliteRequest {
        query: "CREATE TABLE test (id INTEGER PRIMARY KEY);".to_string(),
        params: vec![],
      },
    )
    .await
    .unwrap();

    // Read
    let response = handle_sqlite_query(
      conn.clone(),
      SqliteRequest {
        query: "SELECT * FROM test".to_string(),
        params: vec![],
      },
    )
    .await
    .unwrap();

    let SqliteResponse::Query { rows } = response else {
      panic!("expected query, got: {response:?}");
    };

    assert_eq!(0, rows.len());

    // Attach
    let _ = handle_sqlite_query(
      conn.clone(),
      SqliteRequest {
        query: "ATTACH DATABASE ':memory:' AS foo".to_string(),
        params: vec![],
      },
    )
    .await
    .unwrap();

    // Detach
    let _ = handle_sqlite_query(
      conn.clone(),
      SqliteRequest {
        query: "DETACH DATABASE foo".to_string(),
        params: vec![],
      },
    )
    .await
    .unwrap();

    // Error
    let response = handle_sqlite_query(
      conn.clone(),
      SqliteRequest {
        query: "NOT A VALID QUERY :)".to_string(),
        params: vec![],
      },
    )
    .await
    .unwrap();

    let SqliteResponse::Error(err) = response else {
      panic!("expected error, got: {response:?}");
    };

    assert!(
      err.contains("near \"NOT\": syntax error in NOT A VALID QUERY :) at offset 0"),
      "Got: {err:?}"
    );
  }
}
