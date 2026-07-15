use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
  #[error("HttpStatus: {0}")]
  HttpStatus(reqwest::StatusCode),

  #[error("RecordSerialization: {0}")]
  RecordSerialization(serde_json::Error),

  #[error("InvalidToken: {0}")]
  InvalidToken(jsonwebtoken::errors::Error),

  #[error("InvalidUrl: {0}")]
  InvalidUrl(url::ParseError),

  // NOTE: This error is leaky but comprehensively unpacking reqwest is unsustainable.
  #[error("Reqwest: {0}")]
  OtherReqwest(reqwest::Error),

  #[cfg(feature = "ws")]
  #[error("WebSocket: {0}")]
  WebSocket(#[from] reqwest_websocket::Error),
}

impl From<reqwest::Error> for Error {
  fn from(err: reqwest::Error) -> Self {
    match err.status() {
      Some(code) => Self::HttpStatus(code),
      _ => Self::OtherReqwest(err),
    }
  }
}
