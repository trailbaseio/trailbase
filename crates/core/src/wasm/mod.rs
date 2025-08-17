use axum::Router;
use axum::extract::{RawPathParams, Request};
use bytes::Bytes;
use http_body_util::{BodyExt, combinators::BoxBody};
use hyper::StatusCode;
use std::str::FromStr;
use std::sync::Arc;
use trailbase_wasm::Runtime;
use trailbase_wasm::exports::trailbase::runtime::init_endpoint::MethodType;

use crate::AppState;
use crate::User;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

pub(crate) fn build_wasm_runtime(
  data_dir: &crate::DataDir,
  conn: trailbase_sqlite::Connection,
) -> Result<Option<Runtime>, AnyError> {
  let scripts_dir = data_dir.root().join("scripts");

  if let Ok(entries) = std::fs::read_dir(scripts_dir) {
    for entry in entries {
      let Ok(entry) = entry else {
        continue;
      };

      let Ok(metadata) = entry.metadata() else {
        continue;
      };

      if !metadata.is_file() {
        continue;
      }
      let path = entry.path();
      let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
        continue;
      };

      if extension == "wasm" {
        return Ok(Some(Runtime::new(2, path, conn)?));
      }
    }
  }

  log::debug!("No WASM file found");

  return Ok(None);
}

pub(crate) async fn install_routes_and_jobs(
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
    let name_clone = name.to_string();

    let Some(job) = state.jobs().new_job(
      None,
      name,
      schedule,
      crate::scheduler::build_callback(move || {
        let name = name_clone.clone();
        let runtime = runtime.clone();

        return async move {
          runtime
            .call(async move |instance| -> Result<(), trailbase_wasm::Error> {
              // FIXME: Add a custom scheme handler.
              let request = hyper::Request::builder()
                .uri(format!("http://__job/?name={name}"))
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

  log::debug!("Got {} WASM routes", init_result.http_handlers.len());

  let mut router = Router::<AppState>::new();
  for (method, path) in &init_result.http_handlers {
    let runtime = runtime.clone();
    // FIXME: Wire up user somehow
    let handler = move |_params: RawPathParams, user: Option<User>, req: Request| async move {
      return runtime
        .call(
          async move |instance| -> Result<axum::response::Response, trailbase_wasm::Error> {
            let (parts, body) = req.into_parts();
            let bytes = body
              .collect()
              .await
              .map_err(|_err| trailbase_wasm::Error::ChannelClosed)?
              .to_bytes();

            let response = instance
              .call_incoming_http_handler(hyper::Request::from_parts(
                parts,
                BoxBody::new(http_body_util::Full::new(bytes).map_err(|_| unreachable!())),
              ))
              .await?;

            let (parts, body) = response.into_parts();
            let bytes = body
              .collect()
              .await
              .map_err(|_err| trailbase_wasm::Error::ChannelClosed)?
              .to_bytes();

            return Ok(axum::response::Response::from_parts(parts, bytes.into()));
          },
        )
        .await
        .flatten()
        .unwrap_or_else(|err| {
          return axum::response::Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(err.to_string().into())
            .expect("success");
        });
    };

    log::debug!("Installing WASM route: {method:?}: {path}");

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
