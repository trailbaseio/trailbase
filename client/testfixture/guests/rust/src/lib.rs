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

        let user_id_str = match user_id {
          Value::Blob(b) => BASE64_STANDARD.encode(b),
          x => format!("{x:?}"),
        };

        eprintln!("[print from WASM guest] user id: {user_id_str}");

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
    return Some(Metadata {
      display_name: Some("TestFixture Rust".to_string()),
      icon: Some(ICON.to_string()),
      description: Some("A component used within Trailbase's tests.".to_string()),
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

const ICON: &str = r##"<svg height="800px" width="800px" version="1.1" id="_x32_" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 512 512"  xml:space="preserve">
<g>
  <path class="st0" d="M424.712,0c-13.927-0.017-25.211,11.233-25.228,25.16c-0.016,13.91,11.25,25.193,25.16,25.21
    c13.91,0.017,25.203-11.25,25.219-25.169C449.872,11.292,438.622,0.009,424.712,0z"/>
  <path class="st0" d="M429.087,120.032c0.008-8.193-6.614-14.823-14.789-14.832c-8.192-0.008-14.83,6.622-14.839,14.806
    c0,8.183,6.63,14.822,14.806,14.822C422.457,134.846,429.087,128.208,429.087,120.032z"/>
  <path class="st0" d="M461.241,65.304c-9.781-0.026-17.736,7.888-17.736,17.668c-0.018,9.797,7.913,17.711,17.702,17.736
    c9.764,0,17.719-7.906,17.719-17.694C478.942,73.242,471.02,65.304,461.241,65.304z"/>
  <path class="st0" d="M78.238,395.333c-19.712,19.713-19.712,51.782,0,71.494c19.713,19.713,51.79,19.713,71.503,0l146.434-146.434
    H153.186L78.238,395.333z"/>
  <path class="st0" d="M332.374,121.181c-11.934-11.943-31.36-11.943-43.294,0c-7.72,7.72-10.439,18.564-8.175,28.496l-1.96,1.968
    L56.752,373.839c-31.57,31.562-31.57,82.921,0,114.483c31.554,31.571,82.922,31.571,114.476,0l222.201-222.193l1.96-1.96
    c9.932,2.264,20.785-0.456,28.505-8.175c11.934-11.943,11.943-31.36,0-43.294L332.374,121.181z M381.832,257.159l-57.474,57.482
    L160.957,478.043c-25.946,25.937-67.99,25.937-93.935,0c-25.928-25.937-25.928-67.989,0-93.927l162.599-162.598l58.293-58.277
    l2.787-2.804c0.388,0.422,0.778,0.828,1.182,1.232l91.52,91.52c0.397,0.405,0.81,0.794,1.225,1.182L381.832,257.159z
     M413.606,245.715c-4.333,4.333-10.524,5.667-16.014,4.021l-4.164-4.164l-93.926-93.926l-4.164-4.164
    c-1.656-5.49-0.312-11.689,4.02-16.022c6.276-6.275,16.461-6.275,22.736,0l91.511,91.51
    C419.889,229.254,419.889,239.432,413.606,245.715z"/>
</g>
</svg>"##;
