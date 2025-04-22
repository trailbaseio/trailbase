use axum::body::Body;
use axum::http::{StatusCode, header::CONTENT_TYPE};
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

impl From<trailbase_sqlite::Error> for AuthError {
  fn from(err: trailbase_sqlite::Error) -> Self {
    return match err {
      trailbase_sqlite::Error::Rusqlite(err) => match err {
        rusqlite::Error::QueryReturnedNoRows => {
          #[cfg(debug_assertions)]
          info!("SQLite returned empty rows error");

          Self::NotFound
        }

        rusqlite::Error::SqliteFailure(err, _msg) => {
          match err.extended_code {
            // List of error codes: https://www.sqlite.org/rescode.html
            275 => Self::BadRequest("sqlite constraint: check"),
            531 => Self::BadRequest("sqlite constraint: commit hook"),
            3091 => Self::BadRequest("sqlite constraint: data type"),
            787 => Self::BadRequest("sqlite constraint: fk"),
            1043 => Self::BadRequest("sqlite constraint: function"),
            1299 => Self::BadRequest("sqlite constraint: not null"),
            2835 => Self::BadRequest("sqlite constraint: pinned"),
            1555 => Self::BadRequest("sqlite constraint: pk"),
            2579 => Self::BadRequest("sqlite constraint: row id"),
            1811 => Self::BadRequest("sqlite constraint: trigger"),
            2067 => Self::BadRequest("sqlite constraint: unique"),
            2323 => Self::BadRequest("sqlite constraint: vtab"),
            _ => Self::Internal(err.into()),
          }
        }
        _ => Self::Internal(err.into()),
      },
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
      Self::FailedDependency(err) if cfg!(debug_assertions) => {
        (StatusCode::FAILED_DEPENDENCY, Some(err.to_string()))
      }
      Self::FailedDependency(_err) => (StatusCode::FAILED_DEPENDENCY, None),
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

#[cfg(test)]
mod tests {
  use axum::http::StatusCode;
  use axum::response::IntoResponse;

  use crate::auth::AuthError;

  #[tokio::test]
  async fn test_some_sqlite_errors_yield_client_errors() {
    let conn = trailbase_sqlite::Connection::new(
      || crate::connection::connect_rusqlite_without_default_extensions_and_schemas(None),
      None,
    )
    .unwrap();

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

    match sqlite_err {
      trailbase_sqlite::Error::Rusqlite(rusqlite::Error::SqliteFailure(err, _)) => {
        assert_eq!(err.extended_code, 1555);
      }
      _ => panic!("{sqlite_err}"),
    };

    let err: AuthError = sqlite_err.into();
    assert_eq!(err.into_response().status(), StatusCode::BAD_REQUEST);
  }
}
