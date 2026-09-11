use bytes::Bytes;
use http::Uri;
use http_body_util::{BodyExt, combinators::UnsyncBoxBody};
use sqlite3_parser::ast::{OneSelect, Select, Stmt};
use tokio::time::Duration;
use trailbase_schema::parse::{Bump, parse_into_statement, parse_into_statements};
use trailbase_schema::sqlite::unquote_expr;
use trailbase_sqlite::{LockError, Rows};
use trailbase_sqlvalue::{DecodeError, SqlValue};
use trailbase_wasm_common::{SqliteRequest, SqliteResponse};
use wasmtime_wasi_http::p2::bindings::http::types::ErrorCode;

pub use trailbase_sqlite::OwnedTx;

pub(crate) async fn acquire_transaction_lock_with_timeout(
  conn: trailbase_sqlite::Connection,
  timeout: Duration,
) -> Result<OwnedTx, LockError> {
  let try_until = std::time::SystemTime::now() + timeout;
  loop {
    match conn.try_write_arc_lock_for(Duration::from_micros(50)) {
      Ok(lock) => {
        return OwnedTx::new(lock).map_err(|err| {
          log::error!("Failed to construct OwnedTx: {err}");
          return LockError::NotSupported;
        });
      }
      Err(LockError::Timeout) => {
        // Sleep a little.
        tokio::time::sleep(Duration::from_micros(200)).await;

        if std::time::SystemTime::now() > try_until {
          return Err(LockError::Timeout);
        }
        continue;
      }
      Err(LockError::NotSupported) => {
        return Err(LockError::NotSupported);
      }
    }
  }
}

async fn handle_sqlite_execute(
  conn: trailbase_sqlite::Connection,
  request: SqliteRequest,
) -> Result<SqliteResponse, String> {
  return match Parsed::single_from_query(&request.query)? {
    Parsed::Attach { path, db_name } => {
      validate_attach_statement(&path, &db_name)?;

      conn.attach(&path, &db_name).await.map_err(sqlite_err)?;
      Ok(SqliteResponse::Execute { rows_affected: 0 })
    }
    Parsed::Detach { db_name } => {
      conn.detach(&db_name).await.map_err(sqlite_err)?;
      Ok(SqliteResponse::Execute { rows_affected: 0 })
    }
    Parsed::Empty => Ok(SqliteResponse::Execute { rows_affected: 0 }),
    Parsed::WriteQuery | Parsed::ReadQuery => Ok(SqliteResponse::Execute {
      rows_affected: conn
        .execute(
          request.query.clone(),
          sql_values_to_sqlite_params(request.params).map_err(sqlite_err)?,
        )
        .await
        .map_err(sqlite_err)?,
    }),
  };
}

async fn handle_sqlite_execute_batch(
  conn: trailbase_sqlite::Connection,
  request: SqliteRequest,
) -> Result<SqliteResponse, String> {
  if !request.params.is_empty() {
    return Err("bind not supported".to_string());
  }

  let stmts = Parsed::list_from_query(&request.query)?
    .into_iter()
    .flat_map(|stmt| match stmt {
      Parsed::Attach { .. } | Parsed::Detach { .. } => Some(Err("not supported".to_string())),
      Parsed::Empty => None,
      stmt => Some(Ok(stmt)),
    })
    .collect::<Result<Vec<_>, _>>()?;

  if !stmts.is_empty() {
    conn
      .execute_batch(request.query)
      .await
      .map_err(sqlite_err)?;
  }

  return Ok(SqliteResponse::ExecuteBatch);
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

  fn build_query_response(rows: Rows) -> Result<SqliteResponse, String> {
    let json_rows = rows
      .iter()
      .map(convert_values)
      .collect::<Result<Vec<_>, _>>()?;

    return Ok(SqliteResponse::Query { rows: json_rows });
  }

  return match Parsed::single_from_query(&request.query)? {
    // NOTE: We need to handle connection mutations (attach, detach) specially, so that they
    // apply to all internal read and write connections.
    Parsed::Attach { path, db_name } => {
      if let Err(err) = validate_attach_statement(&path, &db_name) {
        return Ok(SqliteResponse::Error(err));
      }

      conn.attach(&path, &db_name).await.map_err(sqlite_err)?;
      Ok(SqliteResponse::Query { rows: vec![] })
    }
    Parsed::Detach { db_name } => {
      conn.detach(&db_name).await.map_err(sqlite_err)?;
      Ok(SqliteResponse::Query { rows: vec![] })
    }
    Parsed::Empty => Ok(SqliteResponse::Query { rows: vec![] }),
    Parsed::ReadQuery => build_query_response(read(conn, request).await?),
    Parsed::WriteQuery => build_query_response(write(conn, request).await?),
  };
}

pub(crate) async fn handle_sqlite_request(
  conn: trailbase_sqlite::Connection,
  request: http::Request<wasmtime_wasi_http::WasiBody>,
) -> Result<http::Response<wasmtime_wasi_http::WasiBody>, wasmtime_wasi_http::Error> {
  let (uri, sqlite_request) = match to_request(request).await {
    Ok(request) => request,
    Err(err) => {
      return to_response(SqliteResponse::Error(err));
    }
  };

  let response = match uri.path() {
    "/execute" => handle_sqlite_execute(conn, sqlite_request).await,
    "/batch" => handle_sqlite_execute_batch(conn, sqlite_request).await,
    "/query" => handle_sqlite_query(conn, sqlite_request).await,
    _ => {
      // NOTE: Should not happen and doesn't need to be handled by the client as
      // SqliteResponse::Error.
      return Err(wasmtime_wasi_http::Error::InternalError(Some(format!(
        "Invalid path: {uri}"
      ))));
    }
  };

  return match response {
    Ok(response) => to_response(response),
    Err(err) => to_response(SqliteResponse::Error(err)),
  };
}

// NOTE: We need to handle connection mutations (attach, detach, pragmas) specially, so that they
// apply to all internal connections (write & readers).
// NOTE: Unlike `execute` we handle `query`s as potentially read-only or rw. We parse the
// statement below to determine that and allow for cheaper, concurrent reads.
enum Parsed {
  Attach { path: String, db_name: String },
  Detach { db_name: String },
  ReadQuery,
  WriteQuery,
  Empty,
}

impl Parsed {
  fn single_from_query(query: &str) -> Result<Parsed, String> {
    let allocator = Bump::new();
    return Parsed::from_statement(
      parse_into_statement(&allocator, query).map_err(|err| err.to_string())?,
    );
  }

  fn list_from_query(query: &str) -> Result<Vec<Parsed>, String> {
    let allocator = Bump::new();
    let stmts = parse_into_statements(&allocator, query).map_err(|err| err.to_string())?;

    return stmts
      .into_iter()
      .map(|stmt| Parsed::from_statement(Some(stmt)))
      .collect();
  }

  fn from_statement(stmt: Option<Stmt>) -> Result<Parsed, String> {
    return match stmt {
      None => Ok(Parsed::Empty),
      Some(Stmt::Pragma(_, _)) => Err("pragmas not supported".into()),
      Some(Stmt::Attach { expr, db_name, .. }) => Ok(Parsed::Attach {
        path: unquote_expr(&expr),
        db_name: unquote_expr(&db_name),
      }),
      Some(Stmt::Detach(name)) => Ok(Parsed::Detach {
        db_name: unquote_expr(&name),
      }),
      Some(Stmt::Select(select)) if is_readonly_select(select) => Ok(Parsed::ReadQuery),
      _ => Ok(Parsed::WriteQuery),
    };
  }
}

async fn to_request(
  request: http::Request<wasmtime_wasi_http::WasiBody>,
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
) -> Result<http::Response<wasmtime_wasi_http::WasiBody>, wasmtime_wasi_http::Error> {
  let body =
    serde_json::to_vec(&response).map_err(|err| ErrorCode::InternalError(Some(err.to_string())))?;

  let resp = http::Response::builder()
    .status(200)
    .body(bytes_to_body(Bytes::from_owner(body)))
    .map_err(|err| ErrorCode::InternalError(Some(err.to_string())))?;

  return Ok(resp);
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

/// Validates statements like `ATTACH DATABASE {path} AS {db_name}`.
fn validate_attach_statement(path: &str, db_name: &str) -> Result<(), String> {
  const INVALID_NAMES: &[&str] = &["main", "public", "logs", "session"];

  if INVALID_NAMES.contains(&db_name) || db_name.is_empty() {
    return Err(format!("invalid db name: {db_name}"));
  }

  // QUESTION: Should we further validate or constraint the path, e.g. it's not
  // /etc/shadow? At the moment WASM components are pretty trusted.
  if path.is_empty() {
    return Err("path is empty".into());
  }

  for name in INVALID_NAMES {
    if path.contains(&format!("{name}.db")) {
      return Err(format!("invalid path: {path}"));
    }
  }

  return Ok(());
}

#[inline]
pub fn bytes_to_body<E>(bytes: Bytes) -> UnsyncBoxBody<Bytes, E> {
  UnsyncBoxBody::new(http_body_util::Full::new(bytes).map_err(|_| unreachable!()))
}

#[inline]
pub fn sqlite_err<E: std::error::Error>(err: E) -> String {
  return err.to_string();
}

#[allow(clippy::single_match)]
#[inline]
fn is_readonly_select(select: &Select) -> bool {
  fn is_readonly_one_select(select: &OneSelect) -> bool {
    return match select {
      OneSelect::Select { .. } => {
        // for column in columns {
        //   if let ResultColumn::Expr(Expr::FunctionCall { name, .. }, _) = column {
        //     // Filter out SQLean's "define" which is clearly mutating and will
        //     // leave connections in an inconsistent state.
        //     //
        //     // QUESTION: Should we do more, e.g. error and reject the query? It's likely not
        //     // enough to just relegate this to the write connection.
        //     match name.0.as_bytes() {
        //       b"define" | b"undefine" | b"define_free" => {
        //         return false;
        //       }
        //       _ => {}
        //     }
        //   }
        // }

        return true;
      }
      OneSelect::Values(_) => true,
    };
  }

  if let Some(ref with) = select.with {
    for cte in with.ctes {
      if !is_readonly_select(cte.select) {
        return false;
      }
    }
  }

  let body = &select.body;
  if let Some(ref compounds) = body.compounds {
    for compound in compounds {
      if !is_readonly_one_select(&compound.select) {
        return false;
      }
    }
  }

  return is_readonly_one_select(&body.select);
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
    let err = handle_sqlite_execute(
      conn.clone(),
      SqliteRequest {
        query: "NOT A VALID QUERY :)".to_string(),
        params: vec![],
      },
    )
    .await
    .err()
    .unwrap();

    assert!(err.contains("near \"NOT\": syntax error"), "Got: {err:?}");
  }

  #[tokio::test]
  async fn handle_sqlite_execute_batch_test() {
    let conn = Connection::open_in_memory().unwrap();

    let query = "CREATE TABLE IF NOT EXISTS 'test' (id INTEGER PRIMARY KEY)";

    assert!(
      handle_sqlite_execute_batch(
        conn.clone(),
        SqliteRequest {
          query: query.to_string(),
          params: vec![SqlValue::Null],
        },
      )
      .await
      .is_err()
    );

    handle_sqlite_execute_batch(
      conn.clone(),
      SqliteRequest {
        query: format!("{query};{query}"),
        params: vec![],
      },
    )
    .await
    .unwrap();

    // Empty
    handle_sqlite_execute_batch(
      conn.clone(),
      SqliteRequest {
        query: "".to_string(),
        params: vec![],
      },
    )
    .await
    .unwrap();

    // Invalid query.
    assert!(
      handle_sqlite_execute_batch(
        conn.clone(),
        SqliteRequest {
          query: format!("{query}; NOT A VALID QUERY;"),
          params: vec![],
        },
      )
      .await
      .is_err()
    );

    // Connection mutations not allowed
    assert!(
      handle_sqlite_execute_batch(
        conn.clone(),
        SqliteRequest {
          query: format!("{query}; ATTACH DATABASE 'foo.db' AS 'foo';"),
          params: vec![],
        },
      )
      .await
      .is_err()
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
    let err = handle_sqlite_query(
      conn.clone(),
      SqliteRequest {
        query: "NOT A VALID QUERY :)".to_string(),
        params: vec![],
      },
    )
    .await
    .err()
    .unwrap();

    assert!(err.contains("near \"NOT\": syntax error"), "Got: {err:?}");
  }

  fn parse_select<'b>(allocator: &'b Bump, s: &'b str) -> &'b Select<'b> {
    let stmt = parse_into_statement(&allocator, s).unwrap().unwrap();
    if let Stmt::Select(select) = stmt {
      return select;
    }
    panic!("Expected SELECT, got: {stmt:?}");
  }

  #[test]
  fn readonly_select_filter_test() {
    let allocator = Bump::new();
    let select = parse_select(&allocator, "SELECT * FROM test;");
    assert!(is_readonly_select(select));
  }

  #[test]
  fn validate_attach_statement_test() {
    validate_attach_statement("foo.db", "foo").unwrap();

    assert!(validate_attach_statement("", "foo").is_err());
    assert!(validate_attach_statement("foo.db", "main").is_err());
    assert!(validate_attach_statement("foo.db", "").is_err());
    assert!(validate_attach_statement("../session.db", "foo").is_err());
    assert!(validate_attach_statement("foo.db", "session").is_err());
  }
}
