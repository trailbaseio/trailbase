use http::StatusCode;
use serde::de::DeserializeOwned;
use wstd::http::{Client, IntoBody, Request};
use serde_json;

use crate::http::HttpError;

#[derive(Default, Clone, Debug)]
pub struct FetchOptions {
  /// HTTP method, e.g. "GET"/"POST". Default: "GET" when None.
  pub method: Option<String>,
  /// Simple list of headers. Kept as owned strings for simplicity and safety.
  pub headers: Option<Vec<(String, String)>>,
  /// Optional body bytes. `None` means no body (empty body).
  pub body: Option<Vec<u8>>,
}

/// JS-like fetch: `fetch(url, options)`
/// Note: to get raw bytes (Vec<u8>) use `fetch_bytes` or the convenience `get`.
pub async fn fetch<T: DeserializeOwned>(
  uri: impl ToString,
  opts: FetchOptions,
) -> Result<T, HttpError> {
  let bytes = fetch_bytes(uri, opts).await?;

  let parsed: T = serde_json::from_slice(&bytes)
    .map_err(|err| HttpError::message(StatusCode::BAD_REQUEST, err))?;

  Ok(parsed)
}

/// Fetch raw bytes without attempting JSON deserialization.
pub async fn fetch_bytes(uri: impl ToString, opts: FetchOptions) -> Result<Vec<u8>, HttpError> {
  let method = opts.method.as_deref().unwrap_or("GET");

  let mut builder = Request::builder().uri(uri.to_string()).method(method);

  if let Some(headers) = opts.headers.as_ref() {
    for (k, v) in headers.iter() {
      builder = builder.header(k.as_str(), v.as_str());
    }
  }

  let body_bytes = opts.body.unwrap_or_default();

  let request = builder
    .body(body_bytes.into_body())
    .map_err(|err| HttpError::message(StatusCode::BAD_REQUEST, err))?;

  let client = Client::new();

  let (parts, mut body) = client
    .send(request)
    .await
    .map_err(|err| HttpError::message(StatusCode::BAD_REQUEST, err))?
    .into_parts();

  let bytes = body
    .bytes()
    .await
    .map_err(|err| HttpError::message(StatusCode::BAD_REQUEST, err))?;

  if parts.status != StatusCode::OK {
    let text = String::from_utf8_lossy(&bytes).to_string();
    return Err(HttpError::message(parts.status, text));
  }

  Ok(bytes.to_vec())
}

pub async fn get(uri: impl Into<http::Uri>) -> Result<Vec<u8>, HttpError> {
  return fetch_bytes(uri.into(), FetchOptions::default()).await;
}
