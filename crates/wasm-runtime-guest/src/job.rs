use futures_util::future::LocalBoxFuture;

use crate::http::{HttpError, StatusCode};

pub type JobHandler =
  Box<dyn (Fn() -> LocalBoxFuture<'static, Result<(), HttpError>>) + Send + Sync>;

pub struct Job {
  pub name: String,
  pub spec: String,
  pub handler: JobHandler,
}

impl Job {
  // NOTE: We use anyhow here specifically to allow guests to attach context.
  pub fn new<F>(name: impl Into<String>, spec: impl Into<String>, f: F) -> Self
  where
    F: (AsyncFn() -> Result<(), anyhow::Error>) + Send + Sync + 'static,
  {
    let f = std::sync::Arc::new(f);

    return Self {
      name: name.into(),
      spec: spec.into(),
      handler: Box::new(move || {
        let f = f.clone();
        Box::pin(async move {
          f().await.map_err(|err| HttpError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: Some(format!("{err}")),
          })
        })
      }),
    };
  }
}
