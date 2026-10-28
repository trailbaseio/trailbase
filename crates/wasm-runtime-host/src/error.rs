#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Wasmtime: {0}")]
  Wasmtime(#[from] wasmtime::Error),
  #[error("Channel closed")]
  ChannelClosed,
  #[error("Http Error: {0}")]
  HttpErrorCode(wasmtime_wasi_http::p2::bindings::http::types::ErrorCode),
  #[error("Encoding")]
  Encoding,
  #[error("Json")]
  Json(#[from] serde_json::Error),
  #[error("Timeout: {0:?}")]
  Timeout(Option<http::Uri>),
  #[error("Other: {0}")]
  Other(String),
}
