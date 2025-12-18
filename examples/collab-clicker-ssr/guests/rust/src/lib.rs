#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use rquickjs::loader::{BuiltinLoader, BuiltinResolver};
use rquickjs::prelude::{Async, Ctx, Func};
use rquickjs::{
  AsyncContext, AsyncRuntime, Context, Function, Module, Object, Runtime, async_with,
};
use trailbase_wasm::db::{Value, query};
use trailbase_wasm::fs::read_file;
use trailbase_wasm::http::{HttpError, HttpRoute, Json, StatusCode, routing};
use trailbase_wasm::kv::Store;
use trailbase_wasm::time::{Duration, Timer};
use trailbase_wasm::{Guest, export};

// Implement the function exported in this world (see above).
struct Endpoints;

impl Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![
      routing::get("/clicked", async |_req| -> Result<Json<_>, HttpError> {
        let rows = query(
          "UPDATE counter SET value = value + 1 WHERE id = 1 RETURNING value",
          [],
        )
        .await
        .map_err(internal)?;

        return Ok(Json(serde_json::json!({
            "count": get_count(rows.first())?,
        })));
      }),
      routing::get("/", async |_req| -> Result<String, HttpError> {
        // NOTE: this is replicating vite SSR template's server.js;
        let rows = query("SELECT value FROM counter WHERE id = 1", [])
          .await
          .map_err(internal)?;

        let count = get_count(rows.first())?;

        // Call the JS render function using embedded QuickJS.
        let result = render(count).await?;

        let template = read_cached_file("/dist/client/index.html")?;
        let mut template_str = String::from_utf8_lossy(&template).to_string();

        template_str = template_str.replace("<!--app-head-->", &result.head);
        template_str = template_str.replace("<!--app-data-->", &result.data);
        template_str = template_str.replace("<!--app-html-->", &result.html);

        return Ok(template_str);
      }),
      routing::get("/fibonacci", async |req| {
        let n: usize = req
          .query_param("n")
          .and_then(|p| p.parse().ok())
          .unwrap_or(40);

        return Ok(format!("{}\n", fibonacci(n)?));
      }),
    ];
  }
}

fn get_count(row: Option<&Vec<Value>>) -> Result<i64, HttpError> {
  let Some(value) = row.and_then(|r| r.first()) else {
    return Err(internal("count was deleted - very funny :)"));
  };

  let Value::Integer(count) = value else {
    return Err(internal("value is not an int"));
  };

  return Ok(*count);
}

fn read_cached_file(path: &str) -> Result<Vec<u8>, HttpError> {
  let mut store = Store::open().map_err(internal)?;

  let Ok(Some(template)) = store.get(path) else {
    let contents = read_file(path).map_err(internal)?;
    store.set(path, &contents).map_err(internal)?;
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
  callback.call::<_, ()>(()).expect("success");

  Ok(())
}

async fn render(count: i64) -> Result<RenderResult, HttpError> {
  let resolver = BuiltinResolver::default().with_module("server/entry-server.js");
  let module = read_cached_file("/dist/server/entry-server.js")?;
  let loader = BuiltinLoader::default().with_module("server/entry-server.js", module);

  let rt = AsyncRuntime::new().map_err(internal)?;
  rt.set_loader(resolver, loader).await;

  let ctx = AsyncContext::full(&rt).await.map_err(internal)?;
  let result: Result<RenderResult, HttpError> = async_with!(ctx => |ctx| {
    ctx
      .globals()
      .set("setTimeout", Func::from(Async(set_timeout)))
      .map_err(internal)?;

    let (module, promise) = Module::declare(
      ctx.clone(),
      "ssr",
      format!(r#"
        import {{ render }} from "server/entry-server.js";

        const count = {count};
        export const output = render("ignored", count);
      "#),
    )
    .map_err(internal)?
    .eval()
    .map_err(internal)?;

    promise.finish::<()>().map_err(internal)?;

    let obj: Object = module.get("output").map_err(internal)?;

    return Ok(RenderResult {
      head: obj.get("head").map_err(internal)?,
      data: obj.get("data").map_err(internal)?,
      html: obj.get("html").map_err(internal)?,
    });
  })
  .await;

  // Drain event-loop giving pending timers a chance to run.
  rt.idle().await;

  return result;
}

fn fibonacci(n: usize) -> Result<usize, HttpError> {
  let resolver = BuiltinResolver::default();
  let loader = BuiltinLoader::default();

  let rt = Runtime::new().map_err(internal)?;
  rt.set_loader(resolver, loader);

  let ctx = Context::full(&rt).map_err(internal)?;
  return ctx.with(|ctx| -> Result<usize, HttpError> {
    let (module, promise) = Module::declare(
      ctx,
      "fibonacci",
      format!(
        r#"
function fibonacci(num) {{
  switch (num) {{
    case 0:
      return 0;
    case 1:
      return 1;
    default:
      return fibonacci(num - 1) + fibonacci(num - 2);
  }}
}}

export const output = fibonacci({n});
"#
      ),
    )
    .map_err(internal)?
    .eval()
    .map_err(internal)?;

    promise.finish::<()>().map_err(internal)?;

    return module.get("output").map_err(internal);
  });
}

fn internal(err: impl std::string::ToString) -> HttpError {
  return HttpError::message(StatusCode::INTERNAL_SERVER_ERROR, err);
}

export!(Endpoints);
