#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use base64::prelude::*;
use std::sync::atomic::{AtomicI64, Ordering};
use trailbase_wasm::auth::require_admin;
use trailbase_wasm::db::{Transaction, Value, execute, query};
use trailbase_wasm::fetch::{Uri, get};
use trailbase_wasm::fs::read_file;
use trailbase_wasm::http::{HttpError, HttpRoute, Json, StatusCode, routing};
use trailbase_wasm::job::Job;
use trailbase_wasm::sqlite::SqliteFunctionFlag;
use trailbase_wasm::time::{Duration, SystemTime, Timer};
use trailbase_wasm::{Guest, Metadata, SqliteFunction, export};

// Implement the function exported in this world (see above).
struct Endpoints;

static SEQ: AtomicI64 = AtomicI64::new(-32);

impl Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    SEQ.fetch_add(1000, Ordering::SeqCst);

    return vec![
      routing::get("/method", async |_req| Ok("get")),
      routing::post("/method", async |_req| Ok("post")),
      routing::delete("/method", async |_req| Ok("delete")),
      routing::get("/readfile", async |_req| {
        let r = read_file("/crates/sqlite/Cargo.toml")
          .map_err(|err| HttpError::message(StatusCode::NOT_FOUND, err))?;
        eprintln!("result: {}", String::from_utf8_lossy(&r));
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
        eprintln!("waiting {ms}ms");

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

        eprintln!("[print from WASM guest] user id: {user_id:?}");

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

        // Keep one dangling to make sure RAII-cleanup works.
        let _tx_dangling = Transaction::begin();

        return Ok(());
      }),
      routing::get("/attach_db", async |_req| {
        let _ = execute("ATTACH DATABASE foo.db AS foo", vec![])
          .await
          .map_err(internal)?;
        return Ok(());
      }),
      routing::get("/detach_db", async |_req| {
        let _ = query("DETACH DATABASE foo", vec![])
          .await
          .map_err(internal)?;
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
      routing::get("/stateful", async |_req| {
        return Ok(format!("{}\n", SEQ.fetch_add(1, Ordering::SeqCst)));
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
      routing::get("/test_sqlite-vec", async |_req| {
        let Value::Blob(ref vec) = query("SELECT vec_f32('[0, 1, 2, 3]')", vec![])
          .await
          .unwrap()[0][0]
        else {
          return Err(internal("expected blob"));
        };
        return Ok(BASE64_STANDARD.encode(vec));
      }),
      routing::get("/dash", async |req| {
        require_admin(&req).await?;

        return Ok(
          r#"
            <html>
            <body style="background-color:#92a8d1;">
                Hello World

                <button type="button" onclick="test();">
                    alert
                </button>
            </body>
            <script>
                function test() {
                    alert("test");
                }
            </script>
            </html>
          "#,
        );
      }),
    ];
  }

  fn job_handlers() -> Vec<Job> {
    SEQ.fetch_add(4000, Ordering::SeqCst);

    return vec![Job::hourly("WASM-registered Job", async || {
      eprintln!("JS-registered cron job reporting for duty 🚀");
    })];
  }

  fn sqlite_scalar_functions() -> Vec<SqliteFunction> {
    SEQ.fetch_add(32, Ordering::SeqCst);
    return vec![
      SqliteFunction::new::<1>(
        "custom_echo".to_string(),
        |args: [trailbase_wasm::sqlite::Value; _]| {
          return Ok(args[0].clone());
        },
        &[
          SqliteFunctionFlag::Deterministic,
          SqliteFunctionFlag::Innocuous,
        ],
      ),
      SqliteFunction::new::<0>(
        "custom_stateful".to_string(),
        |_args: [trailbase_wasm::sqlite::Value; _]| {
          return Ok(trailbase_wasm::sqlite::Value::Integer(
            SEQ.fetch_add(1, Ordering::SeqCst),
          ));
        },
        &[],
      ),
    ];
  }

  fn metadata() -> Option<Metadata> {
    return Some(Metadata{
      display_name: Some("testfixture_rust".to_string()),
      icon: Some(r###"<svg viewBox="0 0 1024 1024" class="icon" version="1.1" xmlns="http://www.w3.org/2000/svg" fill="#000000"><g id="SVGRepo_bgCarrier" stroke-width="0"></g><g id="SVGRepo_tracerCarrier" stroke-linecap="round" stroke-linejoin="round"></g><g id="SVGRepo_iconCarrier"><path d="M373.2 600.3h278.7v8H373.2z" fill="#999999"></path><path d="M512.6 948.3h-9.8c-58.7 0-106.7-48-106.7-106.7v-212h259v176.2c0 78.4-64.2 142.5-142.5 142.5z" fill="#F9C0C0"></path><path d="M511.7 958.8c-40.7 0-79-15.9-108-44.9s-44.9-67.3-44.9-108V209.2h-32.2c-11.4 0-20.7-9.3-20.7-20.7v-17.6c0-11.4 9.3-20.7 20.7-20.7h370.1c11.4 0 20.7 9.3 20.7 20.7v17.6c0 11.4-9.3 20.7-20.7 20.7h-32.2v596.7c0 40.7-15.9 79-44.9 108-28.9 28.9-67.2 44.9-107.9 44.9zM326.6 165.1c-3.2 0-5.7 2.6-5.7 5.7v17.6c0 3.2 2.6 5.7 5.7 5.7h47.2v611.7c0 36.7 14.4 71.3 40.5 97.4 26.1 26.1 60.7 40.5 97.4 40.5s71.3-14.4 97.4-40.5c26.1-26.1 40.5-60.7 40.5-97.4V194.2h47.2c3.2 0 5.7-2.6 5.7-5.7v-17.6c0-3.2-2.6-5.7-5.7-5.7l-370.2-0.1z" fill="#999999"></path><path d="M373.2 193.8h50.7v8h-50.7zM466.8 193.8h185.1v8H466.8z" fill="#999999"></path><path d="M535.7 558.5c-14.1 0-25.5-11.4-25.5-25.5s11.4-25.5 25.5-25.5 25.5 11.4 25.5 25.5c0 14-11.4 25.5-25.5 25.5z m0-43c-9.6 0-17.5 7.8-17.5 17.5 0 9.6 7.8 17.5 17.5 17.5s17.5-7.8 17.5-17.5-7.9-17.5-17.5-17.5zM458.1 417.6c-21.3 0-38.6-17.3-38.6-38.6s17.3-38.6 38.6-38.6 38.6 17.3 38.6 38.6-17.3 38.6-38.6 38.6z m0-69.2c-16.9 0-30.6 13.7-30.6 30.6s13.7 30.6 30.6 30.6 30.6-13.7 30.6-30.6-13.7-30.6-30.6-30.6zM566.7 107.3c-11.4 0-20.7-9.3-20.7-20.7s9.3-20.7 20.7-20.7 20.7 9.3 20.7 20.7-9.2 20.7-20.7 20.7z m0-33.4c-7 0-12.7 5.7-12.7 12.7s5.7 12.7 12.7 12.7 12.7-5.7 12.7-12.7-5.7-12.7-12.7-12.7zM540.5 299.5c-16.7 0-30.3-13.6-30.3-30.3s13.6-30.3 30.3-30.3 30.3 13.6 30.3 30.3-13.6 30.3-30.3 30.3z m0-52.6c-12.3 0-22.3 10-22.3 22.3s10 22.3 22.3 22.3 22.3-10 22.3-22.3-10-22.3-22.3-22.3z" fill="#CE0202"></path></g></svg>"###.to_string()),
      description: Some("my description".to_string()),
      admin_ui_path: Some("/dash".to_string()),
      ..Default::default()
    });
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
