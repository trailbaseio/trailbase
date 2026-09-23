#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use regex::Regex;
use rquickjs::prelude::{Async, Ctx, Func};
use rquickjs::{AsyncContext, AsyncRuntime, Context, Function, Module, Object, Runtime};
use std::borrow::Cow;
use std::sync::LazyLock;
use trailbase_wasm::db::{Value, query};
use trailbase_wasm::http::{HttpError, HttpRoute, Json, StatusCode, routing};
use trailbase_wasm::time::{Duration, FutureExt, Timer};
use trailbase_wasm::{Guest, export};

// Struct to implement the guest's APIs on.
struct Endpoints;
export!(Endpoints);

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
            "count": get_count(rows).map_err(internal)?,
        })));
      }),
      routing::get("/", async |req| -> Result<String, HttpError> {
        let rows = query("SELECT value FROM counter WHERE id = 1", [])
          .await
          .map_err(internal)?;

        let maybe_count = get_count(rows);

        // Call the JS render function using embedded QuickJS and fill in the template. This is
        // basically replicating vite-ssr-template's server.js;
        let result = render(req.url().as_str(), maybe_count).await?;

        let template: Cow<'_, str> = cfg_select! {
          feature = "bundle" => assets::HTML_TEMPLATE.into(),
          _ => String::from_utf8(read_cached_file("/dist/client/index.html")?)
            .map_err(internal)?
            .into(),
        };

        let template = PLACEHOLDER_RE
          .replace_all(&template, |caps: &regex::Captures| {
            let key = &caps[1];
            return match key {
              "app-head" => &result.head,
              "app-data" => &result.data,
              "app-html" => &result.html,
              _ => unreachable!("template mismatch"),
            };
          })
          .to_string();

        return Ok(template);
      }),
      routing::get("/echo/{input}", async |req| {
        return echo(req.path_param("input").unwrap_or_default()).map_err(internal);
      }),
    ];
  }
}

fn get_count(rows: Vec<Vec<Value>>) -> Result<i64, &'static str> {
  let Some(value) = rows.first().and_then(|r| r.first()) else {
    return Err(GET_COUNT_ERR);
  };

  let Value::Integer(count) = value else {
    return Err("value is not an int");
  };

  return Ok(*count);
}

async fn set_timeout<'js>(
  _ctx: Ctx<'js>,
  callback: Function<'js>,
  timeout_ms: u64,
) -> Result<(), rquickjs::Error> {
  // Wait and then call provided JS callback function.
  Timer::after(Duration::from_nanos(timeout_ms)).wait().await;
  callback.call::<_, ()>(())?;
  return Ok(());
}

#[derive(Debug)]
struct RenderResult {
  head: String,
  data: String,
  html: String,
}

async fn render(
  url: &str,
  maybe_count: Result<i64, &'static str>,
) -> Result<RenderResult, HttpError> {
  let js_module = cfg_select! {
    feature = "bundle" => assets::JS_MODULE,
    _ => read_cached_file("/dist/server/entry-server.js")?,
  };

  let rt = AsyncRuntime::new().map_err(internal)?;
  // rt.set_loader(
  //   BuiltinResolver::default().with_module("server/entry-server.js"),
  //   BuiltinLoader::default().with_module("server/entry-server.js", js_module),
  // )
  // .await;

  let ctx = AsyncContext::full(&rt).await.map_err(internal)?;
  let result = ctx
    .async_with(async |ctx| -> Result<RenderResult, rquickjs::Error> {
      // Register `set_timeout` called by solid-js w/o explicit 0ms timeout.
      ctx.globals().set(
        "setTimeout",
        Func::from(Async(async |ctx, cb| set_timeout(ctx, cb, 0).await)),
      )?;

      let (module, promise) = Module::declare(ctx, "server/entry-server.js", js_module)?.eval()?;
      promise.finish::<()>()?;

      // Call `render()` function from "server/entry-server.js".
      let echo: Function = module.get("render")?;
      let obj: Object = match maybe_count {
        Ok(count) => echo.call((url, count))?,
        Err(err) => echo.call((url, -1, err))?,
      };

      return Ok(RenderResult {
        head: obj.get("head")?,
        data: obj.get("data")?,
        html: obj.get("html")?,
      });
    })
    .await;

  // Drain event-loop giving pending timers a chance to run.
  if let Err(err) = rt.idle().timeout(Duration::from_millis(1000)).await {
    eprintln!("Failed to drain event-loop: {err}");
  };

  return result.map_err(internal);
}

// Only exists to test custom `rquickjs`-guest integration: input + output.
fn echo(s: &str) -> Result<String, rquickjs::Error> {
  let rt = Runtime::new()?;
  return Context::full(&rt)?.with(|ctx| -> Result<String, rquickjs::Error> {
    ctx.eval::<(), _>("function echoFn(input) { return input; }")?;
    let echo: Function = ctx.globals().get("echoFn")?;
    return echo.call((s,));
  });
}

fn internal(err: impl std::string::ToString) -> HttpError {
  return HttpError::message(StatusCode::INTERNAL_SERVER_ERROR, err);
}

#[cfg(not(feature = "bundle"))]
fn read_cached_file(path: &str) -> Result<Vec<u8>, HttpError> {
  let mut store = trailbase_wasm::kv::Store::open().map_err(internal)?;

  let Ok(Some(template)) = store.get(path) else {
    let contents = trailbase_wasm::fs::read_file(path).map_err(internal)?;
    store.set(path, &contents).map_err(internal)?;
    return Ok(contents);
  };

  return Ok(template);
}

const GET_COUNT_ERR: &str =
  "Someone deleted the counter - very funny :) - Will be fixed by periodic reset.";
static PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<!--([\w-]+)-->").unwrap());

#[cfg(feature = "bundle")]
mod assets {
  pub const HTML_TEMPLATE: &'static str = include_str!("../../../dist/client/index.html");
  pub const JS_MODULE: &'static [u8] = include_bytes!("../../../dist/server/entry-server.js");
}
