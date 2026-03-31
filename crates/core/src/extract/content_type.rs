use axum::extract::{FromRequest, Request};
use axum::http::header::{ACCEPT, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use thiserror::Error;

/// Supported request content types.
///
/// We error for unsupported content types and fall back to application/json for unspecified
/// content types.
pub enum RequestContentType {
  // Unknown,
  Json,
  Multipart,
  Form,
}

#[derive(Debug, Error)]
pub enum ContentTypeRejection {
  #[error("Unsupported Content-Type: {0}")]
  UnsupportedContentType(String),
}

impl IntoResponse for ContentTypeRejection {
  fn into_response(self) -> Response {
    return (StatusCode::BAD_REQUEST, self.to_string()).into_response();
  }
}

impl RequestContentType {
  #[inline]
  pub fn from_headers(headers: &HeaderMap) -> Result<Self, ContentTypeRejection> {
    return match headers.get(CONTENT_TYPE).map(|h| h.as_bytes()) {
      Some(content_type) if content_type.starts_with(b"application/json") => {
        Ok(RequestContentType::Json)
      }
      Some(content_type) if content_type.starts_with(b"application/x-www-form-urlencoded") => {
        Ok(RequestContentType::Form)
      }
      Some(content_type) if content_type.starts_with(b"multipart/form-data") => {
        Ok(RequestContentType::Multipart)
      }
      Some(content_type) => Err(ContentTypeRejection::UnsupportedContentType(
        String::from_utf8_lossy(content_type).into(),
      )),
      // QUESTION: Not convinced this is a sensible default for "None" but convenient for testing
      // with curl.
      None => Ok(RequestContentType::Json),
    };
  }
}

pub enum ResponseContentType {
  Json,
}

#[allow(unused)]
impl ResponseContentType {
  pub fn from_headers(headers: &HeaderMap) -> Result<Self, ContentTypeRejection> {
    // We mimic the requests's content type. However, we won't reply in forms.
    if let Ok(_request_content_type) = RequestContentType::from_headers(headers) {
      return Ok(ResponseContentType::Json);
    }

    for value in headers.get_all(ACCEPT) {
      if value == "application/json" {
        return Ok(ResponseContentType::Json);
      }
    }

    return Err(ContentTypeRejection::UnsupportedContentType(
      headers
        .get(CONTENT_TYPE)
        .and_then(|c| c.to_str().map(|c| c.to_string()).ok())
        .unwrap_or_default(),
    ));
  }
}

impl<S> FromRequest<S> for ResponseContentType
where
  S: Send + Sync,
{
  type Rejection = ContentTypeRejection;

  async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
    return Self::from_headers(req.headers());
  }
}
