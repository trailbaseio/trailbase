#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use trailbase_wasm_guest::db::{Value, query};
use trailbase_wasm_guest::fs::read_file;
use trailbase_wasm_guest::kv::Store;
use trailbase_wasm_guest::{HttpError, HttpRoute, Method, export};
use wstd::http::StatusCode;

// Implement the function exported in this world (see above).
struct Endpoints;

impl trailbase_wasm_guest::Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![
      HttpRoute::new(
        Method::GET,
        "/clicked",
        async |_req| -> Result<String, HttpError> {
          const QUERY: &str = "UPDATE counter SET value = value + 1 WHERE id = 1 RETURNING value";
          let rows = query(QUERY.to_string(), vec![]).await.map_err(internal)?;

          let Value::Integer(count) = rows[0][0] else {
            return Err(internal(""));
          };

          return Ok(
            serde_json::to_string(&serde_json::json!({
                "count": count,
            }))
            .unwrap(),
          );
        },
      ),
      HttpRoute::new(
        Method::GET,
        "/",
        async |_req| -> Result<String, HttpError> {
          // NOTE: this is replicating vite SSR template's server.js;
          let rows = query("SELECT value FROM counter WHERE id = 1".to_string(), vec![])
            .await
            .map_err(internal)?;

          let Value::Integer(count) = rows[0][0] else {
            return Err(internal(""));
          };

          // NOTE: This is where we'd run the JS render function if we could.
          let (rendered_head, rendered_data, rendered_html) = ("", "", format!("{count}"));

          let template = get_template().map_err(internal)?;
          let mut template_str = String::from_utf8_lossy(&template).to_string();

          template_str = template_str.replace("<!--app-head-->", rendered_head);
          template_str = template_str.replace("<!--app-html-->", &rendered_html);
          template_str = template_str.replace("<!--app-data-->", rendered_data);

          return Ok(template_str);
        },
      ),
    ];
  }
}

fn get_template() -> Result<Vec<u8>, String> {
  let mut store = Store::open()?;

  let Some(template) = store.get("template") else {
    let contents = read_file("/dist/client/index.html")?;
    store.set("template", &contents);
    return Ok(contents);
  };

  return Ok(template);
}

fn internal(err: impl std::string::ToString) -> HttpError {
  return HttpError {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    message: Some(err.to_string()),
  };
}

export!(Endpoints);
