use futures_util::future::LocalBoxFuture;
use wstd::http::server::{Finished, Responder};

use crate::http::IntoResponse;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("InvalidSpec")]
  InvalidSpec,
}

pub type JobHandler = Box<dyn Fn(Responder) -> LocalBoxFuture<'static, Finished>>;

pub struct Job {
  pub name: String,
  pub spec: String,
  pub handler: JobHandler,
}

impl Job {
  pub fn new<F, R, B>(
    name: impl std::string::ToString,
    spec: impl std::string::ToString,
    f: F,
  ) -> Result<Self, Error>
  where
    F: (AsyncFn() -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: wstd::http::body::Body,
  {
    let spec = spec.to_string();
    validate_spec(&spec)?;

    let f = std::rc::Rc::new(f);

    return Ok(Self {
      name: name.to_string(),
      spec,
      handler: Box::new(move |responder| {
        let f = f.clone();
        Box::pin(async move { responder.respond(f().await.into_response()).await })
      }),
    });
  }

  pub fn minutely<F, R, B>(name: impl std::string::ToString, f: F) -> Self
  where
    F: (AsyncFn() -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: wstd::http::body::Body,
  {
    return Self::new(name, "37 * * * * *", f).expect("valid spec");
  }

  pub fn hourly<F, R, B>(name: impl std::string::ToString, f: F) -> Self
  where
    F: (AsyncFn() -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: wstd::http::body::Body,
  {
    return Self::new(name, "@hourly", f).expect("valid spec");
  }

  pub fn daily<F, R, B>(name: impl std::string::ToString, f: F) -> Self
  where
    F: (AsyncFn() -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: wstd::http::body::Body,
  {
    return Self::new(name, "@daily", f).expect("valid spec");
  }
}

fn validate_spec(spec: &str) -> Result<(), Error> {
  return match spec {
    "@hourly" | "@daily" | "@weekly" | "@monthly" | "@yearly" => Ok(()),
    spec => match spec.trim().split(" ").count() {
      6 | 7 => Ok(()),
      _ => Err(Error::InvalidSpec),
    },
  };
}
