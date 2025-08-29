#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use trailbase_wasm::db::{Transaction, Value, execute, query};
use trailbase_wasm::http::{HttpError, HttpRoute, Method, StatusCode};
use trailbase_wasm::job::Job;
use trailbase_wasm::time::{Duration, Timer};
use trailbase_wasm::{Guest, export, thread_id};

// Implement the function exported in this world (see above).
struct Endpoints;

impl Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![
      HttpRoute::new(Method::GET, "/fibonacci", async |_req| {
        format!("{}\n", fibonacci(40))
      }),
      HttpRoute::new(Method::GET, "/sleep", async |_req| {
        Timer::after(Duration::from_secs(10)).wait().await;
      }),
      HttpRoute::new(Method::GET, "/wasm/{placeholder}", async |req| {
        let url = req.url();
        return Ok(format!("Welcome from WASM [{}]: {url}\n", thread_id()));
      }),
      HttpRoute::new(Method::GET, "/wasm_query", async |_req| {
        execute(
          "CREATE TABLE test (id INTEGER PRIMARY KEY)".to_string(),
          vec![],
        )
        .await
        .map_err(internal)?;

        let _ = execute("INSERT INTO test (id) VALUES (2), (4)".to_string(), vec![]).await;

        let rows = query("SELECT COUNT(*) FROM test".to_string(), vec![])
          .await
          .map_err(internal)?;

        return Ok(format!("rows: {:?}", rows[0][0]).into_bytes().to_vec());
      }),
      HttpRoute::new(Method::GET, "/sqlitetx", async |_req| {
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
      HttpRoute::new(Method::GET, "/sqlitetxread", async |_req| {
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
