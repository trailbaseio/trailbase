#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("ConnectionClosed")]
  ConnectionClosed,

  /// Error when the value of a particular column is requested, but the type
  /// of the result in that column cannot be converted to the requested
  /// Rust type.
  #[error("InvalidColumnType({idx}, {name}, {decl_type:?})")]
  InvalidColumnType {
    idx: usize,
    name: String,
    decl_type: Option<crate::rows::ValueType>,
  },

  #[error("FromSql: {0}")]
  FromSql(#[from] crate::from_sql::FromSqlError),

  // QUESTION: This is leaky. How often do downstream users have to introspect on this
  // rusqlite::Error. Otherwise, should/could this be more opaue.
  #[error("Rusqlite: {0}")]
  Rusqlite(#[from] rusqlite::Error),

  #[error("DeserializeValue: {0}")]
  DeserializeValue(serde_rusqlite::Error),

  /// This one is useful for downstream consumers providing a `Connection` builder returning this
  /// error.
  #[error("Other: {0}")]
  Other(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}
