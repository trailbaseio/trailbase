#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use rquickjs::loader::{BuiltinLoader, BuiltinResolver};
use rquickjs::{Context, Module, Object, Runtime};
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

          let result = render(count);
          println!("Render: {result:?}");

          // NOTE: This is where we'd run the JS render function if we could.

          let template = read_cached_file("/dist/client/index.html").map_err(internal)?;
          let mut template_str = String::from_utf8_lossy(&template).to_string();

          template_str = template_str.replace("<!--app-head-->", &result.head);
          template_str = template_str.replace("<!--app-data-->", &result.data);
          template_str = template_str.replace("<!--app-html-->", &result.html);

          return Ok(template_str);
        },
      ),
    ];
  }
}

fn read_cached_file(path: &str) -> Result<Vec<u8>, String> {
  let mut store = Store::open()?;

  let Some(template) = store.get(path) else {
    let contents = read_file(path)?;
    store.set(path, &contents);
    return Ok(contents);
  };

  return Ok(template);
}

const MODULE: &str = r#"
export function render(uri, count) {
  return {
    head: "",
    data: "",
    html: `count: ${count}`,
  };
}
"#;

#[derive(Debug)]
struct RenderResult {
  head: String,
  data: String,
  html: String,
}

fn render(count: i64) -> RenderResult {
  let resolver = BuiltinResolver::default()
    .with_module("server/entry-server.js")
    .with_module("other")
    .with_module("count");

  let module = read_cached_file("/dist/server/entry-server.js").unwrap();
  // println!("read: {}", String::from_utf8_lossy(&module));

  let loader = BuiltinLoader::default()
    .with_module("server/entry-server.js", MODULE)
    .with_module("other", module)
    .with_module("count", format!("export const count = {count};"));

  let rt = Runtime::new().unwrap();
  let ctx = Context::full(&rt).unwrap();

  rt.set_loader(resolver, loader);

  return ctx.with(|ctx| {
    let (module, promise) = Module::declare(
      ctx,
      "ssr",
      r#"
        import { render } from "server/entry-server.js";
        import { render as r } from "other";
        import { count } from "count";

        const url = "ignored";

        export const output = render(url, count);
      "#,
    )
    .unwrap()
    .eval()
    .unwrap();

    promise
      .finish::<()>()
      .map_err(|err| {
        println!("PROMISE: {err}");
        return err;
      })
      .unwrap();

    let obj: Object = module.get("output").unwrap();

    return RenderResult {
      head: obj.get("head").unwrap(),
      data: obj.get("data").unwrap(),
      html: obj.get("html").unwrap(),
    };
  });
}

fn internal(err: impl std::string::ToString) -> HttpError {
  return HttpError {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    message: Some(err.to_string()),
  };
}

export!(Endpoints);
