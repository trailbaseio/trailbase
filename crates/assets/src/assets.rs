use axum::body::{Body, Bytes};
use axum::http::{self, Request, StatusCode};
use axum::response::Response;
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use tower_service::Service;

type FallbackFn = dyn Fn(&str) -> Option<Response<Body>> + Send + Sync;

#[derive(Clone)]
pub struct AssetService<E: RustEmbed> {
  _phantom: std::marker::PhantomData<E>,
  fallback: Arc<FallbackFn>,
}

impl<E: RustEmbed> AssetService<E> {
  pub fn with_parameters(
    fallback: impl Fn(&str) -> Option<Response<Body>> + Send + Sync + 'static,
  ) -> Self {
    Self {
      _phantom: std::marker::PhantomData,
      fallback: Arc::new(fallback),
    }
  }
}

impl<E: RustEmbed> Service<Request<Body>> for AssetService<E> {
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
      fallback: self.fallback.clone(),
      request: req,
    }
  }
}

pub struct ServeFuture<E: RustEmbed> {
  _phantom: std::marker::PhantomData<E>,
  fallback: Arc<FallbackFn>,
  request: Request<Body>,
}

impl<E: RustEmbed> ServeFuture<E> {
  fn not_found() -> Response<Body> {
    return Response::builder()
      .status(StatusCode::NOT_FOUND)
      .header(http::header::CONTENT_TYPE, "text/html")
      .body(Body::from(NOT_FOUND))
      .unwrap_or_default();
  }

  fn not_allowed() -> Response<Body> {
    return Response::builder()
      .status(StatusCode::METHOD_NOT_ALLOWED)
      .header(http::header::CONTENT_TYPE, "text/plain")
      .body(Body::from("Method not allowed"))
      .unwrap_or_default();
  }
}

impl<E: RustEmbed> Future for ServeFuture<E> {
  type Output = Result<Response<Body>, Infallible>;

  fn poll(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
    if self.request.method() != http::Method::GET {
      return Poll::Ready(Ok(Self::not_allowed()));
    }

    let path: &str = self.request.uri().path().trim_start_matches("/");

    let Some(file) = E::get(path) else {
      if let Some(fb_response) = (self.fallback)(path) {
        return Poll::Ready(Ok(fb_response));
      }
      return Poll::Ready(Ok(Self::not_found()));
    };

    // NOTE: We're not selective on the caching here. We rely on vite creating unique names for the
    // assets except for `index.html`, which is handled by the fallback. This is not a generic
    // solution.
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
