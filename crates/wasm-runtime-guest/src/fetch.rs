use serde::de::DeserializeOwned;
use serde_json;
use wstd::http::{Client, IntoBody};
use wstd::io::empty;

pub use http::{Request, Uri};

fn to_err<E: std::fmt::Display>(e: E) -> wstd::http::Error {
  wstd::http::Error::from(wstd::http::error::WasiHttpErrorCode::InternalError(Some(
    e.to_string(),
  )))
}

pub async fn fetch_json<T: DeserializeOwned, B: wstd::http::Body>(
  request: Request<B>,
) -> Result<T, wstd::http::Error> {
  let bytes = fetch(request).await?;
  let result = serde_json::from_slice(&bytes).map_err(to_err)?;
  Ok(result)
}

pub async fn fetch<B: wstd::http::Body>(request: Request<B>) -> Result<Vec<u8>, wstd::http::Error> {
  let client = Client::new();
  let response = client.send(request).await?;
  return response.into_body().bytes().await;
}

pub async fn get(uri: impl Into<Uri>) -> Result<Vec<u8>, wstd::http::Error> {
  return fetch(
    Request::builder()
      .uri(uri.into())
      .body(empty())
      .expect("static"),
  )
  .await;
}

pub async fn post<B: IntoBody>(uri: impl Into<Uri>, body: B) -> Result<Vec<u8>, wstd::http::Error> {
  return fetch(
    Request::builder()
      .method(http::Method::POST)
      .uri(uri.into())
      .body(body.into_body())
      .expect("static"),
  )
  .await;
}
