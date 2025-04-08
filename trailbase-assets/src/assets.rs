use axum::body::{Body, Bytes};
use axum::http::{self, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use tower_service::Service;

type FallbackFn = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

struct State {
  fallback: Option<FallbackFn>,
  index_file: Option<String>,
}

#[derive(Clone)]
pub struct AssetService<E: RustEmbed + Clone> {
  _phantom: std::marker::PhantomData<E>,
  state: Arc<State>,
}

impl<E: RustEmbed + Clone> AssetService<E> {
  pub fn with_parameters(fallback: Option<FallbackFn>, index_file: Option<String>) -> Self {
    Self {
      _phantom: std::marker::PhantomData,
      state: Arc::new(State {
        fallback,
        index_file,
      }),
    }
  }
}

impl<E: RustEmbed + Clone> Service<Request<Body>> for AssetService<E> {
  type Response = Response<Body>;
  type Error = Infallible;
  type Future = ServeFuture<E>;

  fn poll_ready(
    &mut self,
    _cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: Request<Body>) -> Self::Future {
    ServeFuture {
      _phantom: std::marker::PhantomData,
      state: self.state.clone(),
      request: req,
    }
  }
}

pub struct ServeFuture<E: RustEmbed> {
  _phantom: std::marker::PhantomData<E>,
  state: Arc<State>,
  request: Request<Body>,
}

impl<E: RustEmbed> ServeFuture<E> {
  fn not_found() -> Response<Body> {
    return (StatusCode::NOT_FOUND, NOT_FOUND).into_response();
  }
}

impl<E: RustEmbed> Future for ServeFuture<E> {
  type Output = Result<Response<Body>, Infallible>;

  fn poll(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
    if self.request.method() != http::Method::GET {
      return Poll::Ready(Ok(
        Response::builder()
          .status(StatusCode::METHOD_NOT_ALLOWED)
          .header(http::header::CONTENT_TYPE, "text/plain")
          .body(Body::from("Method not allowed"))
          .unwrap_or_default(),
      ));
    }

    let path: &str = match self.request.uri().path().trim_start_matches("/") {
      // If path is only "/" get index file.
      x if x.is_empty() => self.state.index_file.as_deref().unwrap_or(x),
      x => x,
    };

    #[cfg(test)]
    log::debug!("asset path: {:?}", self.request.uri());

    let Some(file) = E::get(path).or_else(|| {
      self
        .state
        .fallback
        .as_ref()
        .and_then(|fb| fb(path).and_then(|f| E::get(&f)))
    }) else {
      return Poll::Ready(Ok(Self::not_found()));
    };

    let response_builder = Response::builder()
      .header(http::header::CACHE_CONTROL, "public")
      .header(http::header::CACHE_CONTROL, "max-age=604800")
      .header(http::header::CACHE_CONTROL, "immutable")
      .header(http::header::CONTENT_TYPE, file.metadata.mimetype());

    return Poll::Ready(Ok(
      response_builder
        .body(Body::from(cow_to_bytes(file.data)))
        .unwrap_or_default(),
    ));
  }
}

fn cow_to_bytes(cow: Cow<'static, [u8]>) -> Bytes {
  match cow {
    Cow::Borrowed(x) => Bytes::from(x),
    Cow::Owned(x) => Bytes::from(x),
  }
}

const NOT_FOUND: &str = r#"
<!DOCTYPE html>
<html>
  <head>
    <title>404 Not Found</title>
  </head>
  <body>
    <h1>Not Found</h1>

    <p>The requested URL was not found on this server.</p>
  </body>
</html>
"#;
