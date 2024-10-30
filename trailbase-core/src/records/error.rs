use axum::body::Body;
use axum::http::{header::CONTENT_TYPE, StatusCode};
use axum::response::{IntoResponse, Response};
use log::*;
use thiserror::Error;

/// Publicly visible errors of record APIs.
///
/// This error is deliberately opaque and kept very close to HTTP error codes to avoid the leaking
/// of internals and provide a very clear mapping to codes.
/// NOTE: Do not use thiserror's #from, all mappings should be explicit.
#[derive(Debug, Error)]
pub enum RecordError {
  #[error("Api Not Found")]
  ApiNotFound,
  #[error("Api Requires Table")]
  ApiRequiresTable,
  #[error("Record Not Found")]
  RecordNotFound,
  #[error("Forbidden")]
  Forbidden,
  #[error("Bad request: {0}")]
  BadRequest(&'static str),
  #[error("Internal: {0}")]
  Internal(Box<dyn std::error::Error + Send + Sync>),
}

impl From<libsql::Error> for RecordError {
  fn from(err: libsql::Error) -> Self {
    return match err {
      libsql::Error::QueryReturnedNoRows => {
        #[cfg(debug_assertions)]
        info!("libsql returned empty rows error");

        Self::RecordNotFound
      }
      // List of error codes: https://www.sqlite.org/rescode.html
      libsql::Error::SqliteFailure(275, _msg) => Self::BadRequest("sqlite constraint: check"),
      libsql::Error::SqliteFailure(531, _msg) => Self::BadRequest("sqlite constraint: commit hook"),
      libsql::Error::SqliteFailure(3091, _msg) => Self::BadRequest("sqlite constraint: data type"),
      libsql::Error::SqliteFailure(787, _msg) => Self::BadRequest("sqlite constraint: fk"),
      libsql::Error::SqliteFailure(1043, _msg) => Self::BadRequest("sqlite constraint: function"),
      libsql::Error::SqliteFailure(1299, _msg) => Self::BadRequest("sqlite constraint: not null"),
      libsql::Error::SqliteFailure(2835, _msg) => Self::BadRequest("sqlite constraint: pinned"),
      libsql::Error::SqliteFailure(1555, _msg) => Self::BadRequest("sqlite constraint: pk"),
      libsql::Error::SqliteFailure(2579, _msg) => Self::BadRequest("sqlite constraint: row id"),
      libsql::Error::SqliteFailure(1811, _msg) => Self::BadRequest("sqlite constraint: trigger"),
      libsql::Error::SqliteFailure(2067, _msg) => Self::BadRequest("sqlite constraint: unique"),
      libsql::Error::SqliteFailure(2323, _msg) => Self::BadRequest("sqlite constraint: vtab"),
      err => Self::Internal(err.into()),
    };
  }
}

impl IntoResponse for RecordError {
  fn into_response(self) -> Response {
    let (status, body) = match self {
      Self::ApiNotFound => (StatusCode::METHOD_NOT_ALLOWED, None),
      Self::ApiRequiresTable => (StatusCode::METHOD_NOT_ALLOWED, None),
      Self::RecordNotFound => (StatusCode::NOT_FOUND, None),
      Self::Forbidden => (StatusCode::FORBIDDEN, None),
      Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, Some(msg.to_string())),
      Self::Internal(err) if cfg!(debug_assertions) => {
        (StatusCode::INTERNAL_SERVER_ERROR, Some(err.to_string()))
      }
      Self::Internal(_err) => (StatusCode::INTERNAL_SERVER_ERROR, None),
    };

    if let Some(body) = body {
      return Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "text/plain")
        .body(Body::new(body))
        .unwrap();
    }

    return Response::builder()
      .status(status)
      .body(Body::empty())
      .unwrap();
  }
}
