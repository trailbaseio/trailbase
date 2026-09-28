#[derive(Debug, thiserror::Error)]
pub enum JsonError {
  #[error("InfiniteFloat")]
  Finite,
  #[error("ValueNotFound")]
  ValueNotFound,
  #[error("UnsupportedType")]
  NotSupported,
  #[error("ColumnMismatch")]
  ColumnMismatch,
  #[error("Decoding")]
  Decode(#[from] base64::DecodeError),
  #[error("UnexpectedT: {0}, expected {1:?}")]
  UnexpectedType(&'static str, crate::db::sqlite::ColumnDataType),
  #[error("ParseInt: {0}")]
  ParseInt(#[from] std::num::ParseIntError),
  #[error("ParseFloat: {0}")]
  ParseFloat(#[from] std::num::ParseFloatError),
  #[error("Serde: {0}")]
  Serde(#[from] serde_json::Error),
  #[cfg(feature = "geos")]
  #[error("Geos: {0}")]
  Geos(#[from] geos::Error),
}
