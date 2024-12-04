#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("Connection closed error")]
  ConnectionClosed,

  /// An error occured while closing the SQLite connection.
  /// This `Error` variant contains the [`Connection`], which can be used to retry the close
  /// operation and the underlying [`rusqlite::Error`] that made it impossible to close the
  /// database.
  #[error("Close error: {1}")]
  Close(crate::connection::Connection, rusqlite::Error),

  #[error("Rusqlite error: {0}")]
  Rusqlite(#[from] rusqlite::Error),

  #[error("SerdeRusqlite error: {0}")]
  SerdeRusqlite(#[from] serde_rusqlite::Error),

  #[error("Other error: {0}")]
  Other(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}
