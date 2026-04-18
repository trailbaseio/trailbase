use axum::body::Body;
use axum::http::{StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use thiserror::Error;

// FIXME: Admin APIs also deserve more explicit error handling eventually.
#[derive(Debug, Error)]
pub enum AdminError {
  #[error("TrailbaseSqlite error: {0}")]
  TrailbaseSqlite(#[from] trailbase_sqlite::Error),
  #[error("Rusqlite: {0}")]
  Rusqlite(#[from] rusqlite::Error),
  #[error("Connection: {0}")]
  Connection(#[from] crate::connection::ConnectionError),
  #[error("FromSql: {0}")]
  FromSql(#[from] trailbase_sqlite::from_sql::FromSqlError),
  #[error("Deserialization: {0}")]
  Deserialization(#[from] serde::de::value::Error),
  #[error("JsonSerialization: {0}")]
  JsonSerialization(#[from] serde_json::Error),
  #[error("Base64 decoding: {0}")]
  Base64Decode(#[from] base64::DecodeError),
  #[error("Already exists: {0}")]
  AlreadyExists(&'static str),
  #[error("Bad request: {0}")]
  BadRequest(Box<dyn std::error::Error + Send + Sync>),
  #[error("precondition failed: {0}")]
  Precondition(String),
  #[error("Internal: {0}")]
  Internal(Box<dyn std::error::Error + Send + Sync>),
  #[error("Schema: {0}")]
  Schema(#[from] trailbase_schema::sqlite::SchemaError),
  #[error("TableLookup: {0}")]
  TableLookup(#[from] crate::schema_metadata::SchemaLookupError),
  #[error("DbMigration: {0}")]
  Migration(#[from] trailbase_refinery::Error),
  #[error("SQL -> Json: {0}")]
  Json(#[from] trailbase_schema::json::JsonError),
  #[error("Schema: {0}")]
  SchemaError(#[from] trailbase_schema::Error),
  #[error("Json -> SQL Params: {0}")]
  Params(#[from] crate::records::params::ParamsError),
  #[error("Config: {0}")]
  Config(#[from] crate::config::ConfigError),
  #[error("Auth: {0}")]
  Auth(#[from] crate::auth::AuthError),
  #[error("WhereClause: {0}")]
  WhereClause(#[from] crate::listing::WhereClauseError),
  #[error("Transaction: {0}")]
  Transaction(#[from] crate::transaction_recorder::TransactionError),
  #[error("JSON schema: {0}")]
  JSONSchema(#[from] crate::schema_metadata::JsonSchemaError),
  #[error("Email: {0}")]
  Email(#[from] crate::email::EmailError),
  #[error("Record: {0}")]
  Record(#[from] crate::records::RecordError),
  #[error("File: {0}")]
  File(#[from] crate::records::files::FileError),
  #[error("SqlValueDecode: {0}")]
  SqlValueDecode(#[from] trailbase_sqlvalue::DecodeError),
}

impl IntoResponse for AdminError {
  fn into_response(self) -> Response {
    let (status, msg) = match self {
      // NOTE: For error types that already implement "into_response" we should just unpack them.
      // We should be able to use a generic for that.
      Self::Auth(err) => return err.into_response(),
      Self::Record(err) => return err.into_response(),
      Self::Deserialization(err) => (StatusCode::BAD_REQUEST, err.to_string()),
      Self::Precondition(_) => (StatusCode::PRECONDITION_FAILED, self.to_string()),
      Self::BadRequest(err) => (StatusCode::BAD_REQUEST, err.to_string()),
      Self::Internal(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
      Self::AlreadyExists(_) => (StatusCode::CONFLICT, self.to_string()),
      // NOTE: We can almost always leak the internal error (except for permission errors) since
      // these are errors for the admin apis.
      err => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    };

    return Response::builder()
      .status(status)
      .header(CONTENT_TYPE, "text/plain")
      .body(Body::new(msg))
      .unwrap_or_default();
  }
}
