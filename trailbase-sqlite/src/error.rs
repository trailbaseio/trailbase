#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("Rusqlite error: {0}")]
  Rusqlite(#[from] rusqlite::Error),

  #[error("SerdeRusqlite error: {0}")]
  SerdeRusqlite(#[from] serde_rusqlite::Error),

  #[error("Other error: {0}")]
  Other(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}
