use axum::Router;
use axum::extract::{RawPathParams, Request};
use bytes::Bytes;
use http_body_util::{BodyExt, combinators::BoxBody};
use std::str::FromStr;
use std::sync::Arc;
use trailbase_wasm::Runtime;
use trailbase_wasm::exports::trailbase::runtime::init_endpoint::MethodType;

use crate::AppState;
use crate::User;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

async fn install_routes_and_jobs(
  state: &AppState,
  runtime: Arc<Runtime>,
) -> Result<Option<Router<AppState>>, AnyError> {
  let init_result = runtime
    .call(async |instance| {
      return instance.call_init().await;
    })
    .await??;

  for (name, spec) in &init_result.job_handlers {
    let schedule = cron::Schedule::from_str(spec)?;
    let runtime = runtime.clone();

    let Some(job) = state.jobs().new_job(
      None,
      name,
      schedule,
      crate::scheduler::build_callback(move || {
        let runtime = runtime.clone();

        return async move {
          runtime
            .call(async move |instance| -> Result<(), trailbase_wasm::Error> {
              // FIXME: Add a custom scheme handler.
              let request = hyper::Request::builder()
                .uri("https://www.rust-lang.org/")
                .body(BoxBody::new(
                  http_body_util::Full::new(Bytes::from_static(b"")).map_err(|_| unreachable!()),
                ))
                .expect("constant");

              instance.call_incoming_http_handler(request).await?;

              return Ok(());
            })
            .await??;

          Ok::<_, AnyError>(())
        };
      }),
    ) else {
      return Err("Failed to add job".into());
    };

    job.start();
  }

  let mut router = Router::<AppState>::new();
  for (method, path) in &init_result.http_handlers {
    let runtime = runtime.clone();
    let handler = move |params: RawPathParams, user: Option<User>, req: Request| async move {
      let result = runtime
        .call(async move |instance| -> Result<(), trailbase_wasm::Error> {
          // FIXME: Add a custom scheme handler.
          let request = hyper::Request::builder()
            .uri("https://www.rust-lang.org/")
            .body(BoxBody::new(
              http_body_util::Full::new(Bytes::from_static(b"")).map_err(|_| unreachable!()),
            ))
            .expect("constant");

          instance.call_incoming_http_handler(request).await?;

          return Ok(());
        })
        .await;

      if let Err(err) = result {
        log::debug!("{err}");
      }
    };

    router = router.route(
      path,
      match method {
        MethodType::Delete => axum::routing::delete(handler),
        MethodType::Get => axum::routing::get(handler),
        MethodType::Head => axum::routing::head(handler),
        MethodType::Options => axum::routing::options(handler),
        MethodType::Patch => axum::routing::patch(handler),
        MethodType::Post => axum::routing::post(handler),
        MethodType::Put => axum::routing::put(handler),
        MethodType::Trace => axum::routing::trace(handler),
      },
    );
  }

  return Ok(Some(router));
}
