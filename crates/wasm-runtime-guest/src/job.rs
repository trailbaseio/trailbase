use futures_util::future::LocalBoxFuture;
use wstd::http::StatusCode;

use crate::http::HttpError;

pub type JobHandler =
  Box<dyn (Fn() -> LocalBoxFuture<'static, Result<(), HttpError>>) + Send + Sync>;

// NOTE: We use anyhow here specifically to allow guests to attach context.
pub fn job_handler(
  f: impl (AsyncFn() -> Result<(), anyhow::Error>) + Send + Sync + 'static,
) -> JobHandler {
  let f = std::sync::Arc::new(f);
  return Box::new(move || {
    let f = f.clone();
    Box::pin(async move {
      f().await.map_err(|err| HttpError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: Some(format!("{err}")),
      })
    })
  });
}
