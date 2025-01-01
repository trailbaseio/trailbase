use axum::extract::{rejection::*, Form, FromRequest, Request};
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use log::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;
use trailbase_sqlite::schema::FileUploadInput;

use crate::extract::multipart::{parse_multipart, Rejection as MultipartRejection};

#[derive(Debug, Error)]
pub enum EitherRejection {
  // #[error("Missing Content-Type")]
  // MissingContentType,
  #[error("Unsupported Content-Type found")]
  UnsupportedContentType,
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

// NOTE: For serde_json::Value as T, the different formats will produce very different results,
// e.g. json has a notion of types, whereas Multipart and Form don't. They're s practically a:
//   Map<String, String | Vec<String>>
#[derive(Debug)]
pub enum Either<T> {
  Json(T),
  Multipart(T, Vec<FileUploadInput>),
  Form(T),
  // Proto(DynamicMessage),
}

impl<S, T> FromRequest<S> for Either<T>
where
  T: DeserializeOwned + Sync + Send + 'static,
  S: Send + Sync,
{
  type Rejection = EitherRejection;

  async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
    return match req.headers().get(CONTENT_TYPE) {
      Some(x) if x.as_ref().starts_with(b"application/json") => {
        let Json(value): Json<T> = Json::from_request(req, state).await?;
        Ok(Either::Json(value))
      }
      Some(x) if x.as_ref().starts_with(b"application/x-www-form-urlencoded") => {
        let Form(value): Form<T> = Form::from_request(req, state).await?;
        Ok(Either::Form(value))
      }
      Some(x) if x.as_ref().starts_with(b"multipart/form-data") => {
        let (value, files) = parse_multipart(req).await?;
        Ok(Either::Multipart(value, files))
      }
      // Some(x) if x == "application/x-protobuf" => {
      //   return Ok(Either::Proto(DynamicMessage::decode::from_request(req,
      // state).await.unwrap())); }
      Some(_) => Err(EitherRejection::UnsupportedContentType),
      None => {
        // TODO: Not convinced this is a sensible default for "None" but convenient for testing with
        // curl.
        let Json(value): Json<T> = Json::from_request(req, state).await?;
        Ok(Either::Json(value))
        // Err(EitherRejection::MissingContentType),
      }
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
      .header("content-type", "application/json; boundary=fieldB")
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
