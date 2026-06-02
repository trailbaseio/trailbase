use oauth2::{AsyncHttpClient, HttpClientError, HttpRequest, HttpResponse};
use std::future::Future;
use std::pin::Pin;

pub struct ReqwestClient(pub reqwest::Client);

// Yanked from oauth2's `reqwest::Client` implementation.
impl<'c> AsyncHttpClient<'c> for ReqwestClient {
  type Error = HttpClientError<reqwest::Error>;

  #[cfg(target_arch = "wasm32")]
  type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, Self::Error>> + 'c>>;
  #[cfg(not(target_arch = "wasm32"))]
  type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, Self::Error>> + Send + Sync + 'c>>;

  fn call(&'c self, request: HttpRequest) -> Self::Future {
    Box::pin(async move {
      let response = self
        .0
        .execute(request.try_into().map_err(Box::new)?)
        .await
        .map_err(Box::new)?;

      let mut builder = axum::http::Response::builder().status(response.status());

      #[cfg(not(target_arch = "wasm32"))]
      {
        builder = builder.version(response.version());
      }

      for (name, value) in response.headers().iter() {
        builder = builder.header(name, value);
      }

      builder
        .body(response.bytes().await.map_err(Box::new)?.to_vec())
        .map_err(HttpClientError::Http)
    })
  }
}
