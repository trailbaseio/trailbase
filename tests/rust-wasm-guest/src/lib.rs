#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use trailbase_wasm_guest::db::{Value, execute, query};
use trailbase_wasm_guest::{
  HttpError, HttpHandler, JobHandler, Method, export, http_handler, job_handler, thread_id,
};
use wstd::http::StatusCode;
use wstd::time::{Duration, Timer};

// Implement the function exported in this world (see above).
struct Endpoints;

fn map_err(err: impl std::error::Error) -> HttpError {
  return HttpError {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    message: Some(err.to_string()),
  };
}

impl trailbase_wasm_guest::Guest for Endpoints {
  fn http_handlers() -> Vec<(Method, &'static str, HttpHandler)> {
    return vec![
      (
        Method::GET,
        "/fibonacci",
        http_handler(async |_req| Ok(format!("{}\n", fibonacci(40)).as_bytes().to_vec())),
      ),
      (
        Method::GET,
        "/sleep",
        http_handler(async |_req| {
          Timer::after(Duration::from_secs(10)).wait().await;

          Ok(b"".to_vec())
        }),
      ),
      (
        Method::GET,
        "/wasm/{placeholder}",
        http_handler(async |req| {
          let url = req.uri();
          return Ok(format!("Welcome from WASM [{}]: {url}\n", thread_id()).into_bytes());
        }),
      ),
      (
        Method::GET,
        "/wasm_query",
        http_handler(async |_req| {
          execute(
            "CREATE TABLE test (id INTEGER PRIMARY KEY)".to_string(),
            vec![],
          )
          .await
          .map_err(map_err)?;
          let _ = execute("INSERT INTO test (id) VALUES (2), (4)".to_string(), vec![]).await;
          let rows = query("SELECT COUNT(*) FROM test".to_string(), vec![])
            .await
            .map_err(map_err)?;

          return Ok(format!("rows: {:?}", rows[0][0]).into_bytes().to_vec());
        }),
      ),
      (
        Method::GET,
        "/sqlitetx",
        http_handler(async |_req| {
          let mut tx = trailbase_wasm_guest::db::Transaction::begin().map_err(map_err)?;
          tx.execute(
            "CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY)",
            &[],
          )
          .map_err(map_err)?;
          let rows_affected = tx
            .execute("INSERT INTO test (id) VALUES (?1)", &[Value::Integer(2)])
            .map_err(map_err)?;
          assert_eq!(1, rows_affected);
          tx.commit().map_err(map_err)?;

          return Ok(b"".to_vec());
        }),
      ),
    ];
  }

  fn job_handlers() -> Vec<(&'static str, &'static str, JobHandler)> {
    return vec![(
      "myjobhandler",
      "@hourly",
      job_handler(async || {
        println!("My jobhandler");
        return Ok(());
      }),
    )];
  }
}

export!(Endpoints);

#[inline]
fn fibonacci(n: usize) -> usize {
  return match n {
    0 => 0,
    1 => 1,
    n => fibonacci(n - 1) + fibonacci(n - 2),
  };
}
