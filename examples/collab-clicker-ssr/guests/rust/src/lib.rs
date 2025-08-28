#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use rquickjs::loader::{BuiltinLoader, BuiltinResolver};
use rquickjs::prelude::{Async, Ctx, Func};
use rquickjs::{AsyncContext, AsyncRuntime, Function, Module, Object, async_with};
use trailbase_wasm_guest::db::{Value, query};
use trailbase_wasm_guest::fs::read_file;
use trailbase_wasm_guest::kv::Store;
use trailbase_wasm_guest::time::{Duration, Timer};
use trailbase_wasm_guest::{Guest, HttpError, HttpRoute, Method, StatusCode, export};

// Implement the function exported in this world (see above).
struct Endpoints;

impl Guest for Endpoints {
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

          // Call the JS render function using embedded QuickJS.
          let result = render(count).await;

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

#[derive(Debug)]
struct RenderResult {
  head: String,
  data: String,
  html: String,
}

// NOTE: SolidJS calls `setTimeout` without a `millis` argument just to yield, however rquickjs
// doesn't seem to care for variadic functions even if argument is `Option`.
async fn set_timeout<'js>(
  _ctx: Ctx<'js>,
  callback: Function<'js>,
  // millis: Option<usize>,
) -> rquickjs::Result<()> {
  Timer::after(Duration::from_millis(0)).wait().await;
  callback.call::<_, ()>(()).unwrap();

  Ok(())
}

async fn render(count: i64) -> RenderResult {
  let resolver = BuiltinResolver::default().with_module("server/entry-server.js");

  let module = read_cached_file("/dist/server/entry-server.js").unwrap();

  let loader = BuiltinLoader::default().with_module("server/entry-server.js", module);

  let rt = AsyncRuntime::new().unwrap();
  let ctx = AsyncContext::full(&rt).await.unwrap();

  rt.set_loader(resolver, loader).await;

  let result: RenderResult = async_with!(ctx => |ctx| {
    ctx
      .globals()
      .set("setTimeout", Func::from(Async(set_timeout)))
      .unwrap();

    let (module, promise) = Module::declare(
      ctx.clone(),
      "ssr",
      format!(r#"
        import {{ render }} from "server/entry-server.js";

        const count = {count};
        export const output = render("ignored", count);
      "#),
    )
    .unwrap()
    .eval()
    .unwrap();

    promise.finish::<()>().unwrap();

    let obj: Object = module.get("output").unwrap();

    return RenderResult {
      head: obj.get("head").unwrap(),
      data: obj.get("data").unwrap(),
      html: obj.get("html").unwrap(),
    };
  })
  .await;

  // Drain event-loop giving pending timers a chance to run.
  rt.idle().await;

  return result;
}

fn internal(err: impl std::string::ToString) -> HttpError {
  return HttpError::message(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
}

export!(Endpoints);
