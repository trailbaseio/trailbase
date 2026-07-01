use thiserror::Error;

use crate::auth::jwt::JwtHelperError;

#[derive(Debug, Error)]
pub enum InitError {
  #[error("SQLite error: {0}")]
  Sqlite(#[from] trailbase_sqlite::Error),
  #[error("Connection error: {0}")]
  Connection(#[from] crate::connection::ConnectionError),
  #[error("IO error: {0}")]
  IO(#[from] std::io::Error),
  #[error("Config error: {0}")]
  Config(#[from] crate::config::ConfigError),
  #[error("JwtHelper error: {0}")]
  JwtHelper(#[from] JwtHelperError),
  #[error("CreateAdmin error: {0}")]
  CreateAdmin(String),
  #[error("Custom initializer error: {0}")]
  CustomInit(String),
  #[error("Table error: {0}")]
  TableError(#[from] crate::schema_metadata::SchemaLookupError),
  #[error("Schema error: {0}")]
  SchemaError(#[from] trailbase_schema::Error),
  #[error("Script error: {0}")]
  ScriptError(String),
  #[error("ObjectStore error: {0}")]
  ObjectStore(#[from] object_store::Error),
  #[error("Auth error: {0}")]
  Auth(#[from] crate::auth::AuthError),
}
