use axum::body::Body;
use axum::http::{StatusCode, header::CONTENT_TYPE};
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

impl From<trailbase_sqlite::Error> for RecordError {
  fn from(err: trailbase_sqlite::Error) -> Self {
    return match err {
      trailbase_sqlite::Error::Rusqlite(err) => match err {
        rusqlite::Error::QueryReturnedNoRows => {
          #[cfg(debug_assertions)]
          info!("SQLite returned empty rows error");

          Self::RecordNotFound
        }

        rusqlite::Error::SqliteFailure(err, _msg) => {
          match err.extended_code {
            // List of error codes: https://www.sqlite.org/rescode.html
            1 => {
              // This generic error is returned, e.g. when a CHECK constraint like `is_json` is
              // violated on insert/update. Surprisingly that's not a 275 :shrug:.
              // Note that we currently validate input with a `json_schema` constraint twice. First
              // when constructing the SqlValue::Text input parameter and then in the actual SQLite
              // extension function, i.e. a CHECK violation should be caught early. For everything
              // else error on the side of a client-error.
              #[cfg(debug_assertions)]
              log::debug!("generic SQLite error code=1: {err}");

              Self::BadRequest("rejected")
            }
            275 => Self::BadRequest("db constraint: check"),
            531 => Self::BadRequest("db constraint: commit hook"),
            3091 => Self::BadRequest("db constraint: data type"),
            787 => Self::BadRequest("db constraint: fk"),
            1043 => Self::BadRequest("db constraint: function"),
            1299 => Self::BadRequest("db constraint: not null"),
            2835 => Self::BadRequest("db constraint: pinned"),
            1555 => Self::BadRequest("db constraint: pk"),
            2579 => Self::BadRequest("db constraint: row id"),
            1811 => Self::BadRequest("db constraint: trigger"),
            2067 => Self::BadRequest("db constraint: unique"),
            2323 => Self::BadRequest("db constraint: vtab"),
            _ => Self::Internal(err.into()),
          }
        }
        _ => Self::Internal(err.into()),
      },
      err => Self::Internal(err.into()),
    };
  }
}

impl From<object_store::Error> for RecordError {
  fn from(err: object_store::Error) -> Self {
    return RecordError::Internal(err.into());
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
        .unwrap_or_default();
    }

    return Response::builder()
      .status(status)
      .body(Body::empty())
      .unwrap_or_default();
  }
}
