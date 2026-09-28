use axum::body::Body;
use axum::http::header::{self, HeaderValue};
use axum::response::{IntoResponse, Response};

#[derive(Debug)]
pub struct RawJson(pub Box<serde_json::value::RawValue>);

impl IntoResponse for RawJson {
  fn into_response(self) -> Response {
    let body: Box<str> = self.0.into();

    return Response::builder()
      .header(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
      )
      .body(Body::from(String::from(body)))
      .expect("valid");
  }
}
