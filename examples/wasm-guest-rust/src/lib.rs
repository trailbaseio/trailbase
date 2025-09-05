#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use trailbase_wasm::db::query;
use trailbase_wasm::http::{HttpError, HttpRoute, Json, Request, StatusCode, routing};
use trailbase_wasm::job::Job;
use trailbase_wasm::time::{Duration, Timer};
use trailbase_wasm::{Guest, export};

// Implement the function exported in this world (see above).
struct Endpoints;

impl Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![
      routing::get("/fibonacci", async |req| {
        let n: usize = req
          .query_param("n")
          .and_then(|p| p.parse().ok())
          .unwrap_or(40);
        return Ok(format!("{}\n", fibonacci(n)));
      }),
      routing::get("/json", json_handler),
      routing::post("/json", json_handler),
      routing::get("/a", async |_| {
        return Ok(vec![b'a'; 5000]);
      }),
      routing::get("/sleep", async |req| {
        let ms: u64 = req
          .query_param("ms")
          .and_then(|p| p.parse().ok())
          .unwrap_or(500);
        Timer::after(Duration::from_millis(ms)).wait().await;
      }),
      routing::get("/count/{table}", async |_req| {
        // const table = req.getPathParam("table");
        let rows = query("SELECT COUNT(*) FROM test", [])
          .await
          .map_err(internal)?;

        return Ok(format!("rows: {:?}", rows[0][0]).into_bytes().to_vec());
      }),
    ];
  }

  fn job_handlers() -> Vec<Job> {
    return vec![Job::minutely("myjob", async || {
      println!("Hello Job");
    })];
  }
}

export!(Endpoints);

async fn json_handler(mut req: Request) -> Json<serde_json::Value> {
  if let Ok(json) = req.body().json::<serde_json::Value>().await {
    return Json(json);
  }

  return Json(serde_json::json!({
      "int": 5,
      "real": 4.2,
      "msg": "foo",
      "obj": {
        "nested": true,
      },
  }));
}

#[inline]
fn fibonacci(n: usize) -> usize {
  return match n {
    0 => 0,
    1 => 1,
    n => fibonacci(n - 1) + fibonacci(n - 2),
  };
}

fn internal(err: impl std::string::ToString) -> HttpError {
  return HttpError::message(StatusCode::INTERNAL_SERVER_ERROR, err);
}
