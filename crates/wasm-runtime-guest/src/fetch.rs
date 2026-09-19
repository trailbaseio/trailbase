use bytes::Bytes;
use wstd::http::{BodyExt, Client};

pub use http::{Request, Uri};

// Unfortunately, wstd just uses anyhow. Try to not leak internals and bring some structure back :/.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Send: {0}")]
  Send(anyhow::Error),
  #[error("Receive: {0}")]
  Receive(anyhow::Error),
}

pub async fn fetch<B: Into<wstd::http::Body>>(request: Request<B>) -> Result<bytes::Bytes, Error> {
  let client = Client::new();
  let response = client.send(request).await.map_err(Error::Send)?;

  return Ok(
    response
      .into_body()
      .into_boxed_body()
      .collect()
      .await
      .map_err(Error::Receive)?
      .to_bytes(),
  );
}

pub async fn get(uri: impl Into<http::Uri>) -> Result<Bytes, Error> {
  return fetch(
    Request::builder()
      .uri(uri.into())
      .body(())
      .unwrap_or_default(),
  )
  .await;
}
