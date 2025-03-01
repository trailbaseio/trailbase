use axum::body::Body;
use axum::http::{header::CONTENT_TYPE, StatusCode};
use axum::response::{IntoResponse, Response};
use log::*;
use thiserror::Error;

// FIXME: Admin APIs also deserve more explicit error handling eventually.
#[derive(Debug, Error)]
pub enum AdminError {
  #[error("TokioRusqlite error: {0}")]
  TokioRusqlite(#[from] trailbase_sqlite::Error),
  #[error("Rusqlite error: {0}")]
  Rusqlite(#[from] rusqlite::Error),
  #[error("Rusqlite FromSql error: {0}")]
  FromSql(#[from] rusqlite::types::FromSqlError),
  #[error("Deserialization error: {0}")]
  Deserialization(#[from] serde::de::value::Error),
  #[error("JsonSerialization error: {0}")]
  JsonSerialization(#[from] serde_json::Error),
  #[error("Base64 decoding error: {0}")]
  Base64Decode(#[from] base64::DecodeError),
  #[error("Already exists: {0}")]
  AlreadyExists(&'static str),
  #[error("Bad request: {0}")]
  BadRequest(Box<dyn std::error::Error + Send + Sync>),
  #[error("precondition failed: {0}")]
  Precondition(String),
  #[error("Internal error: {0}")]
  Internal(Box<dyn std::error::Error + Send + Sync>),
  #[error("Schema error: {0}")]
  Schema(#[from] crate::schema::SchemaError),
  #[error("Table lookup error: {0}")]
  TableLookup(#[from] crate::table_metadata::TableLookupError),
  #[error("DB Migration error: {0}")]
  Migration(#[from] trailbase_refinery_core::Error),
  #[error("SQL -> Json error: {0}")]
  Json(#[from] crate::records::sql_to_json::JsonError),
  #[error("Schema error: {0}")]
  SchemaError(#[from] trailbase_sqlite::schema::SchemaError),
  #[error("Json -> SQL Params error: {0}")]
  Params(#[from] crate::records::json_to_sql::ParamsError),
  #[error("Config error: {0}")]
  Config(#[from] crate::config::ConfigError),
  #[error("Auth error: {0}")]
  Auth(#[from] crate::auth::AuthError),
  #[error("WhereClause error: {0}")]
  WhereClause(#[from] crate::listing::WhereClauseError),
  #[error("Transaction error: {0}")]
  Transaction(#[from] crate::transaction::TransactionError),
  #[error("JSON schema error: {0}")]
  JSONSchema(#[from] crate::table_metadata::JsonSchemaError),
  #[error("Email error: {0}")]
  Email(#[from] crate::email::EmailError),
  #[error("Query error: {0}")]
  Query(#[from] crate::records::json_to_sql::QueryError),
  #[error("File error: {0}")]
  File(#[from] crate::records::files::FileError),
}

impl IntoResponse for AdminError {
  fn into_response(self) -> Response {
    let (status, msg) = match self {
      // FIXME: For error types that already implement "into_response" we should just unpack them.
      // We should be able to use a generic for that.
      Self::Auth(err) => return err.into_response(),
      Self::Deserialization(err) => (StatusCode::BAD_REQUEST, err.to_string()),
      Self::Precondition(_) => (StatusCode::PRECONDITION_FAILED, self.to_string()),
      Self::BadRequest(err) => (StatusCode::BAD_REQUEST, err.to_string()),
      Self::Internal(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
      Self::AlreadyExists(_) => (StatusCode::CONFLICT, self.to_string()),
      // NOTE: We can almost always leak the internal error (except for permission errors) since
      // these are errors for the admin apis.
      ref _err => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
    };

    return Response::builder()
      .status(status)
      .header(CONTENT_TYPE, "text/plain")
      .body(Body::new(msg))
      .unwrap();
  }
}
