#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use base64::prelude::*;
use std::sync::atomic::{AtomicI64, Ordering};
use trailbase_wasm::db::{Transaction, Value, execute, query};
use trailbase_wasm::fetch::{Uri, get};
use trailbase_wasm::fs::read_file;
use trailbase_wasm::http::{HttpError, HttpRoute, Json, StatusCode, routing};
use trailbase_wasm::job::Job;
use trailbase_wasm::time::{Duration, SystemTime, Timer};
use trailbase_wasm::{Guest, SqliteFunction, export, sqlite::SqliteFunctionFlags};

// Implement the function exported in this world (see above).
struct Endpoints;

impl Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![
      routing::get("/readfile", async |_req| {
        let r = read_file("/crates/sqlite/Cargo.toml")
          .map_err(|err| HttpError::message(StatusCode::NOT_FOUND, err))?;
        println!("result: {}", String::from_utf8_lossy(&r));
        return Ok(());
      }),
      routing::get("/json", async |_req| {
        let value = serde_json::json!({
            "int": 5,
            "real": 4.2,
            "msg": "foo",
            "obj": {
              "nested": true,
            },
        });

        return Json(value);
      }),
      routing::get("/fetch", async |req| {
        if let Some(url) = req.query_param("url") {
          let uri: Uri = Uri::try_from(url).map_err(internal)?;
          return get(uri).await.map_err(internal);
        }

        return Err(HttpError::message(
          StatusCode::BAD_REQUEST,
          "Missing ?url= param",
        ));
      }),
      routing::get("/error", async |_req| -> Result<(), HttpError> {
        return Err(HttpError {
          status: StatusCode::IM_A_TEAPOT,
          message: Some("I'm a teapot".to_string()),
        });
      }),
      routing::get("/await", async |req| -> Result<Vec<u8>, HttpError> {
        let ms: u64 = req.query_param("ms").map_or(10, |p| p.parse().unwrap());

        Timer::after(Duration::from_millis(ms)).wait().await;
        return Ok(vec![b'A'; 5000]);
      }),
      // Test Database interactions
      routing::get("/addDeletePost", async |_req| {
        let user_id = &query(
          "SELECT id FROM _user WHERE email = 'admin@localhost'",
          vec![],
        )
        .await
        .map_err(internal)?[0][0];

        println!("[print from WASM guest] user id: {user_id:?}");

        let mut bytes: [u8; 32] = [0; 32];
        trailbase_wasm::rand::get_random_bytes(&mut bytes);

        let body = format!(
          "{now:?} - {rand}",
          now = SystemTime::now(),
          rand = String::from_utf8_lossy(&bytes),
        );

        let num_insertions = execute(
          "INSERT INTO post (author, title, body) VALUES (?1, 'title' , ?2)",
          vec![user_id.clone(), Value::Text(body.clone())],
        )
        .await
        .unwrap();

        let num_deletions = execute(
          "DELETE FROM post WHERE body = ?1",
          vec![Value::Text(body.clone())],
        )
        .await
        .unwrap();

        return if num_insertions == num_deletions {
          Ok("Ok")
        } else {
          Ok("Fail")
        };
      }),
      routing::get("/transaction", async |_req| {
        let mut tx = Transaction::begin().map_err(internal)?;
        tx.execute(
          "CREATE TABLE IF NOT EXISTS tx (id INTEGER PRIMARY KEY)",
          &[],
        )
        .map_err(internal)?;

        let rows = tx.query("SELECT COUNT(*) FROM tx", &[]).map_err(internal)?;
        let Value::Integer(count) = &rows[0][0] else {
          return Err(internal("expected int"));
        };

        let rows_affected = tx
          .execute(
            "INSERT INTO tx (id) VALUES (?1)",
            &[Value::Integer(count + 1)],
          )
          .map_err(internal)?;

        assert_eq!(1, rows_affected);

        tx.commit().map_err(internal)?;

        return Ok(());
      }),
      // Benchmark runtime performance.
      routing::get("/fibonacci", async |req| {
        let n: usize = req.query_param("n").map_or(40, |p| p.parse().unwrap());
        return format!("{}\n", fibonacci(n));
      }),
      routing::get("/sqlite_echo", async |_req| {
        let Value::Integer(i) = &query("SELECT custom_echo(?1)", vec![Value::Integer(5)])
          .await
          .map_err(internal)?[0][0]
        else {
          panic!("Expected Integer");
        };
        assert_eq!(5, *i);

        return Ok(format!("{i}\n"));
      }),
      routing::get("/sqlite_stateful", async |_req| {
        let Value::Integer(i) = &query("SELECT custom_stateful()", vec![])
          .await
          .map_err(internal)?[0][0]
        else {
          panic!("Expected Integer");
        };
        return Ok(format!("{i}\n"));
      }),
      routing::get("/panic", async |_req| {
        if true {
          panic!("/panic called");
        }
        return Ok(());
      }),
      routing::get("/test_sqlean", async |_req| {
        // sqlean: Define a stored procedure, use it, and remove it.
        let _ = query("SELECT define('sumn', ':n * (:n + 1) / 2')", vec![])
          .await
          .unwrap();

        let Value::Integer(value) = query("SELECT sumn(5)", vec![]).await.unwrap()[0][0] else {
          return Err(internal("expected int"));
        };

        let _ = query("SELECT undefine('sumn')", vec![]).await.unwrap();

        return Ok(format!("{value}"));
      }),
      routing::get("/test_sqlite-vec", async |_req| {
        let Value::Blob(ref vec) = query("SELECT vec_f32('[0, 1, 2, 3]')", vec![])
          .await
          .unwrap()[0][0]
        else {
          return Err(internal("expected blob"));
        };
        return Ok(BASE64_STANDARD.encode(vec));
      }),
    ];
  }

  fn job_handlers() -> Vec<Job> {
    return vec![Job::hourly("WASM-registered Job", async || {
      println!("JS-registered cron job reporting for duty 🚀");
    })];
  }

  fn sqlite_scalar_functions() -> Vec<SqliteFunction> {
    return vec![
      SqliteFunction::new::<1>(
        "custom_echo".to_string(),
        |args: [trailbase_wasm::sqlite::Value; _]| {
          return Ok(args[0].clone());
        },
        &[
          SqliteFunctionFlags::Deterministic,
          SqliteFunctionFlags::Innocuous,
        ],
      ),
      SqliteFunction::new::<0>(
        "custom_stateful".to_string(),
        |_args: [trailbase_wasm::sqlite::Value; _]| {
          static COUNT: AtomicI64 = AtomicI64::new(0);
          let curr = COUNT.fetch_add(1, Ordering::SeqCst);
          return Ok(trailbase_wasm::sqlite::Value::Integer(curr));
        },
        &[],
      ),
    ];
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
  return HttpError::message(StatusCode::INTERNAL_SERVER_ERROR, err);
}
