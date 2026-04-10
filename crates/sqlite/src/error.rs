#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("ConnectionClosed")]
  ConnectionClosed,

  // QUESTION: This is leaky. How often do downstream users have to introspect on this
  // rusqlite::Error. Otherwise, should/could this be more opaue.
  #[error("Rusqlite: {0}")]
  Rusqlite(#[from] rusqlite::Error),

  #[error("DeserializeValue: {0}")]
  DeserializeValue(serde_rusqlite::Error),

  /// This one is useful for downstream consumers providin a `Connection` builder returning this
  /// error.
  #[error("Other: {0}")]
  Other(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}
