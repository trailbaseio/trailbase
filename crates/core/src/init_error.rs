use thiserror::Error;

use crate::auth::jwt::JwtHelperError;

#[derive(Debug, Error)]
pub enum InitError {
  #[error("SQLite: {0}")]
  Sqlite(#[from] trailbase_sqlite::Error),
  #[error("Parse: {0}")]
  Parse(Box<dyn std::error::Error + Send + Sync>),
  #[error("Connection: {0}")]
  Connection(#[from] crate::connection::ConnectionError),
  #[error("IO: {0}")]
  IO(#[from] std::io::Error),
  #[error("Config: {0}")]
  Config(#[from] crate::config::ConfigError),
  #[error("JwtHelper: {0}")]
  JwtHelper(#[from] JwtHelperError),
  #[error("CreateAdmin: {0}")]
  CreateAdmin(String),
  #[error("CustomInitializer: {0}")]
  CustomInit(String),
  #[error("Table: {0}")]
  TableError(#[from] crate::schema_metadata::SchemaLookupError),
  #[error("JsonSchema: {0}")]
  JsonSchema(#[from] trailbase_schema::json_schema::Error),
  #[error("Script: {0}")]
  ScriptError(Box<dyn std::error::Error + Send + Sync>),
  #[error("ObjectStore: {0}")]
  ObjectStore(#[from] object_store::Error),
  #[error("Auth: {0}")]
  Auth(#[from] crate::auth::AuthError),
}

impl From<serde_json::Error> for InitError {
  fn from(err: serde_json::Error) -> Self {
    return InitError::Parse(err.into());
  }
}

impl From<base64::DecodeError> for InitError {
  fn from(err: base64::DecodeError) -> Self {
    return InitError::Parse(err.into());
  }
}
