#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use serde_json::json;
use trailbase_wasm_guest::db::{Value, execute, query};
use trailbase_wasm_guest::fetch::{Uri, get};
use trailbase_wasm_guest::fs::read_file;
use trailbase_wasm_guest::time::{Duration, SystemTime, Timer};
use trailbase_wasm_guest::{
  Guest, HttpError, HttpRoute, JobConfig, Method, StatusCode, export, job_handler,
};

// Implement the function exported in this world (see above).
struct Endpoints;

impl Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![
      HttpRoute::new(Method::GET, "/readfile", async |_req| {
        let Ok(r) = read_file("/crates/sqlite/Cargo.toml") else {
          return Err(HttpError {
            status: StatusCode::NOT_FOUND,
            message: Some("file not found".into()),
          });
        };
        println!("result: {}", String::from_utf8_lossy(&r));
        return Ok(());
      }),
      HttpRoute::new(Method::GET, "/json", async |_req| {
        let value = json!({
            "int": 5,
            "real": 4.2,
            "msg": "foo",
            "obj": {
              "nested": true,
            },
        });

        return serde_json::to_vec(&value).map_err(internal);
      }),
      HttpRoute::new(Method::GET, "/fetch", async |req| {
        for (param, value) in req.url().query_pairs() {
          if param == "url" {
            let uri: Uri = Uri::try_from(value.to_string()).map_err(internal)?;
            return get(uri).await.map_err(internal);
          }
        }

        return Err(HttpError::message(
          StatusCode::BAD_REQUEST,
          "Missing ?url= param",
        ));
      }),
      HttpRoute::new(
        Method::GET,
        "/error",
        async |_req| -> Result<(), HttpError> {
          return Err(HttpError {
            status: StatusCode::IM_A_TEAPOT,
            message: Some("I'm a teapot".to_string()),
          });
        },
      ),
      HttpRoute::new(Method::GET, "/await", async |_req| {
        Timer::after(Duration::from_millis(100)).wait().await;
      }),
      // Test Database interactions
      HttpRoute::new(Method::GET, "/addDeletePost", async |_req| {
        let ref user_id = query(
          "SELECT id FROM _user WHERE email = 'admin@localhost'".to_string(),
          vec![],
        )
        .await
        .map_err(internal)?[0][0];

        println!("user id: {user_id:?}");

        let now = SystemTime::now();
        let num_insertions = execute(
          "INSERT INTO post (author, title, body) VALUES (?1, 'title' , ?2)".to_string(),
          vec![user_id.clone(), Value::Text(format!("{now:?}"))],
        )
        .await
        .unwrap();

        let num_deletions = execute(
          "DELETE FROM post WHERE body = ?1".to_string(),
          vec![Value::Text(format!("{now:?}"))],
        )
        .await
        .unwrap();

        if num_insertions != num_deletions {
          panic!("{num_insertions} insertions vs {num_deletions} deletions");
        }

        return Ok("Ok");
      }),
      // Benchmark runtime performance.
      HttpRoute::new(Method::GET, "/fibonacci", async |_req| {
        format!("{}\n", fibonacci(40))
      }),
    ];
  }

  fn job_handlers() -> Vec<JobConfig> {
    return vec![JobConfig {
      name: "WASM-registered Job".into(),
      spec: "@hourly".into(),
      handler: job_handler(async || {
        println!("JS-registered cron job reporting for duty 🚀");
        return Ok(());
      }),
    }];
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
