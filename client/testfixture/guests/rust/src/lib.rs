#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use serde_json::json;
use trailbase_wasm_guest::db::{Value, execute, query};
use trailbase_wasm_guest::{
  HttpError, HttpHandler, JobHandler, Method, export, http_handler, job_handler,
};
use wstd::http::StatusCode;
use wstd::time::{Duration, Timer};

// Implement the function exported in this world (see above).
struct Endpoints;

impl trailbase_wasm_guest::Guest for Endpoints {
  fn http_handlers() -> Vec<(Method, &'static str, HttpHandler)> {
    return vec![
      (
        Method::GET,
        "/json",
        http_handler(async |_req| {
          return Ok(
            serde_json::to_vec(&json!({
            "int": 5,
            "real": 4.2,
            "msg": "foo",
            "obj": {
              "nested": true,
            },

                      }))
            .unwrap(),
          );
        }),
      ),
      (
        Method::GET,
        "/test",
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
    ];
  }

  fn job_handlers() -> Vec<(&'static str, &'static str, JobHandler)> {
    return vec![(
      "myjobhandler",
      "@hourly",
      job_handler(async || {
        println!("My JobHandler");
        return Ok(());
      }),
    )];
  }
}

export!(Endpoints);

// #[inline]
// fn fibonacci(n: usize) -> usize {
//   return match n {
//     0 => 0,
//     1 => 1,
//     n => fibonacci(n - 1) + fibonacci(n - 2),
//   };
// }

fn map_err(err: impl std::error::Error) -> HttpError {
  return HttpError {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    message: Some(err.to_string()),
  };
}
