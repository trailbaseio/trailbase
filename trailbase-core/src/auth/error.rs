use axum::body::Body;
use axum::http::{header::CONTENT_TYPE, StatusCode};
use axum::response::{IntoResponse, Response};
use log::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
  #[error("Unauthorized")]
  Unauthorized,
  #[error("Unauthorized")]
  UnauthorizedExt(Box<dyn std::error::Error + Send + Sync>),
  #[error("Forbidden")]
  Forbidden,
  #[error("Conflict")]
  Conflict,
  #[error("NotFound")]
  NotFound,
  #[error("OAuth provider not found")]
  OAuthProviderNotFound,
  #[error("Bad request: {0}")]
  BadRequest(&'static str),
  #[error("Failed dependency: {0}")]
  FailedDependency(Box<dyn std::error::Error + Send + Sync>),
  #[error("Internal: {0}")]
  Internal(Box<dyn std::error::Error + Send + Sync>),
}

impl From<libsql::Error> for AuthError {
  fn from(err: libsql::Error) -> Self {
    return match err {
      libsql::Error::QueryReturnedNoRows => Self::NotFound,
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

impl IntoResponse for AuthError {
  fn into_response(self) -> Response {
    let (status, body) = match self {
      Self::Unauthorized => (StatusCode::UNAUTHORIZED, None),
      Self::UnauthorizedExt(msg) if cfg!(debug_assertions) => {
        (StatusCode::UNAUTHORIZED, Some(msg.to_string()))
      }
      Self::UnauthorizedExt(_msg) => (StatusCode::UNAUTHORIZED, None),
      Self::Forbidden => (StatusCode::FORBIDDEN, None),
      Self::Conflict => (StatusCode::CONFLICT, None),
      Self::NotFound => (StatusCode::NOT_FOUND, None),
      Self::OAuthProviderNotFound => (StatusCode::METHOD_NOT_ALLOWED, None),
      Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, Some(msg.to_string())),
      Self::FailedDependency(msg) => (StatusCode::FAILED_DEPENDENCY, Some(msg.to_string())),
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

#[cfg(test)]
mod tests {
  use axum::http::StatusCode;
  use axum::response::IntoResponse;

  use crate::auth::AuthError;

  #[tokio::test]
  async fn test_some_sqlite_errors_yield_client_errors() {
    let conn = trailbase_sqlite::connect_sqlite(None, None).await.unwrap();

    conn
      .execute(
        r#"CREATE TABLE test_table (
        id            INTEGER PRIMARY KEY NOT NULL,
        data          TEXT
    );"#,
        (),
      )
      .await
      .unwrap();

    conn
      .execute("INSERT INTO test_table (id, data) VALUES (0, 'first');", ())
      .await
      .unwrap();

    let sqlite_err = conn
      .execute(
        "INSERT INTO test_table (id, data) VALUES (0, 'second');",
        (),
      )
      .await
      .err()
      .unwrap();

    assert!(matches!(sqlite_err, libsql::Error::SqliteFailure(1555, _)));

    let err: AuthError = sqlite_err.into();
    assert_eq!(err.into_response().status(), StatusCode::BAD_REQUEST);
  }
}
