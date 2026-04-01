use axum::Json;
use axum::extract::{Form, FromRequest, Request, rejection::*};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use trailbase_schema::FileUploadInput;

use crate::extract::content_type::{ContentTypeRejection, RequestContentType};
use crate::extract::multipart::{Rejection as MultipartRejection, parse_multipart};

#[derive(Debug, Error)]
pub enum EitherRejection {
  // #[error("Missing Content-Type")]
  // MissingContentType,
  #[error("Unsupported Content-Type: {0}")]
  UnsupportedContentType(String),
  #[error("Form error: {0}")]
  Form(#[from] FormRejection),
  #[error("Json error: {0}")]
  Json(#[from] JsonRejection),
  #[error("Multipart error: {0}")]
  Multipart(#[from] MultipartRejection),
}

impl IntoResponse for EitherRejection {
  fn into_response(self) -> Response {
    return (StatusCode::BAD_REQUEST, format!("{self:?}")).into_response();
  }
}

/// Deserialization helper to support requests in multiple formats.
///
/// Eventually, we'd like to support Avro as well. In which case, we might have to delay
/// de-serialization to pass a schema or we'll only be able to support generic:
/// `Map<string, long | string, ...>` types, which may still provide some compression benefits
/// :shrug:.
#[derive(Debug)]
pub enum Either<T> {
  Json(T),
  Multipart(T, Vec<FileUploadInput>),
  Form(T),
}

impl<S, T> FromRequest<S> for Either<T>
where
  T: DeserializeOwned + Sync + Send + 'static,
  S: Send + Sync,
{
  type Rejection = EitherRejection;

  async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
    return match RequestContentType::from_headers(req.headers()) {
      Ok(RequestContentType::Json) => {
        Ok(Either::Json(Json::<T>::from_request(req, state).await?.0))
      }
      Ok(RequestContentType::Form) => {
        let Form(value): Form<T> = Form::from_request(req, state).await?;
        Ok(Either::Form(value))
      }
      Ok(RequestContentType::Multipart) => {
        let (value, files) = parse_multipart(req).await?;
        Ok(Either::Multipart(value, files))
      }
      Err(err) => match err {
        ContentTypeRejection::UnsupportedContentType(v) => {
          Err(EitherRejection::UnsupportedContentType(v))
        }
      },
    };
  }
}

impl<T> IntoResponse for Either<T>
where
  T: Serialize,
{
  fn into_response(self) -> Response {
    match self {
      Either::Json(json) => axum::Json(json).into_response(),
      Either::Multipart(form, _files) => {
        // Fixme: We would probably have to grab for multer (what Axum uses under the hood). But
        // also not sure, server->client comms as a multipart form is even makes sense.
        axum::Json(form).into_response()
      }
      Either::Form(form) => axum::Form(form).into_response(),
    }
  }
}

#[cfg(test)]
mod test {
  use super::*;
  use indoc::indoc;
  use serde::Serialize;

  #[derive(Debug, Serialize)]
  struct Request {
    value: u16,
  }

  async fn handler(req: Either<Request>) -> (u16, String) {
    return match req {
      Either::Json(r) => (r.value, "JSON".to_string()),
      _ => (0, "ERROR".to_string()),
    };
  }

  #[tokio::test]
  async fn test_handler() {
    let (code, _msg) = handler(Either::Json(Request { value: 42 })).await;
    assert_eq!(code, 42);
  }

  #[tokio::test]
  async fn test_from_request_for_multipart() -> Result<(), anyhow::Error> {
    let body = indoc! {r#"
        --fieldB
        Content-Disposition: form-data; name="name"

        test
        --fieldB
        Content-Disposition: form-data; name="file1"; filename="a.txt"
        Content-Type: text/plain

        Some text
        --fieldB--
      "#}
    .replace("\n", "\r\n");

    let request = axum::http::Request::builder()
      .header("content-type", "multipart/form-data; boundary=fieldB")
      .header("content-length", body.len())
      .body(axum::body::Body::from(body))
      .unwrap();

    let e = Either::<serde_json::Value>::from_request(request, &()).await?;

    assert!(matches!(e, Either::Multipart(..)));

    return Ok(());
  }

  #[tokio::test]
  async fn test_from_request_for_json() -> Result<(), anyhow::Error> {
    let body = indoc! {r#"
      {
        "foo": 42,
        "bar": ["a", "b"]
      }
    "#};

    let request = axum::http::Request::builder()
      .header("ContenT-tYpe", "application/json")
      .header("content-length", body.len())
      .body(axum::body::Body::from(body))
      .unwrap();

    let e = Either::<serde_json::Value>::from_request(request, &()).await?;

    assert!(matches!(e, Either::Json(..)));

    if let Either::Json(value) = e {
      assert_eq!(
        value,
        serde_json::json!({
          "foo": 42,
          "bar": vec!["a", "b"],
        })
      );
    }

    return Ok(());
  }

  #[tokio::test]
  async fn test_from_request_for_urlencoding() -> Result<(), anyhow::Error> {
    let input = serde_json::json!({
      "foo": 42,
      "bar": "a",
      "baz": "b",
    });

    let body = serde_urlencoded::to_string(&input)?;

    let request = axum::http::Request::builder()
      .header("content-type", "application/x-www-form-urlencoded")
      .header("content-length", body.len())
      .method("POST")
      .body(axum::body::Body::from(body))
      .unwrap();

    let e = Either::<serde_json::Value>::from_request(request, &()).await?;

    let Either::Form(value) = e else {
      panic!("{e:?}");
    };

    assert_eq!(
      value,
      serde_json::json!({
        "foo": "42",
        "bar": "a",
        "baz": "b",
      })
    );

    return Ok(());
  }
}
