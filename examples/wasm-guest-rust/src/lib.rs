#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use trailbase_wasm::db::{Transaction, Value, execute, query};
use trailbase_wasm::http::{HttpError, HttpRoute, StatusCode, routing};
use trailbase_wasm::job::Job;
use trailbase_wasm::time::{Duration, Timer};
use trailbase_wasm::{Guest, export, thread_id};

// Implement the function exported in this world (see above).
struct Endpoints;

impl Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![
      routing::get("/fibonacci", async |_req| format!("{}\n", fibonacci(40))),
      routing::get("/sleep", async |_req| {
        Timer::after(Duration::from_secs(10)).wait().await;
      }),
      routing::get("/wasm/{placeholder}", async |req| {
        let url = req.url();
        return Ok(format!("Welcome from WASM [{}]: {url}\n", thread_id()));
      }),
      routing::get("/wasm_query", async |_req| {
        execute("CREATE TABLE test (id INTEGER PRIMARY KEY)", [])
          .await
          .map_err(internal)?;

        let _ = execute("INSERT INTO test (id) VALUES (2), (4)", []).await;

        let rows = query("SELECT COUNT(*) FROM test", [])
          .await
          .map_err(internal)?;

        return Ok(format!("rows: {:?}", rows[0][0]).into_bytes().to_vec());
      }),
      routing::get("/sqlitetx", async |_req| {
        let mut tx = Transaction::begin().map_err(internal)?;
        tx.execute(
          "CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY)",
          &[],
        )
        .map_err(internal)?;
        let rows_affected = tx
          .execute("INSERT INTO test (id) VALUES (?1)", &[Value::Integer(2)])
          .map_err(internal)?;
        assert_eq!(1, rows_affected);
        tx.commit().map_err(internal)?;

        return Ok(b"".to_vec());
      }),
      routing::get("/sqlitetxread", async |_req| {
        let mut tx = Transaction::begin().map_err(internal)?;
        tx.execute(
          "CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY)",
          &[],
        )
        .map_err(internal)?;
        let rows = tx
          .query("SELECT COUNT(*) FROM test", &[])
          .map_err(internal)?;
        tx.commit().map_err(internal)?;

        return Ok(format!("{:?}", rows[0][0]).into_bytes().to_vec());
      }),
    ];
  }

  fn job_handlers() -> Vec<Job> {
    return vec![Job::new("myjobhandler", "@hourly", async || {
      println!("My jobhandler");
      return Ok(());
    })];
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

fn internal(err: impl std::string::ToString) -> HttpError {
  return HttpError::message(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
}
