#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use trailbase_wasm_guest::db::Value;
use trailbase_wasm_guest::{HttpHandler, JobHandler, Method, export, http_handler, job_handler};

// Implement the function exported in this world (see above).
struct Endpoints;

impl trailbase_wasm_guest::Guest for Endpoints {
  fn http_handlers() -> Vec<(Method, &'static str, HttpHandler)> {
    let thread_id = trailbase_wasm_guest::thread_id();
    println!("http_handlers() called (thread: {thread_id})");

    return vec![
      (
        Method::GET,
        "/wasm/{placeholder}",
        http_handler(async |req| {
          let url = req.uri();
          return Ok(format!("Welcome from WASM: {url}\n").into_bytes());
        }),
      ),
      (
        Method::GET,
        "/fibonacci",
        http_handler(async |_req| Ok(format!("{}\n", fibonacci(40)).as_bytes().to_vec())),
      ),
      (
        Method::GET,
        "/sqlitetx",
        http_handler(async |_req| {
          let mut tx = trailbase_wasm_guest::db::Transaction::begin().unwrap();
          tx.execute("CREATE TABLE test (id INTEGER PRIMARY KEY)", &[])
            .unwrap();
          let rows_affected = tx
            .execute("INSERT INTO test (id) VALUES (?1)", &[Value::Integer(2)])
            .unwrap();
          assert_eq!(1, rows_affected);
          tx.commit().unwrap();

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
