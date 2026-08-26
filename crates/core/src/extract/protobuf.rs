//! Protocol Buffer extractor and response.
//!
//! From https://github.com/tokio-rs/axum/blob/main/axum-extra/src/protobuf.rs. Pulled out to update prost.

use axum::http::StatusCode;
use axum::{
  RequestExt,
  extract::{FromRequest, Request},
  response::{IntoResponse, Response},
};
use bytes::BytesMut;
use http_body_util::BodyExt;
use prost::Message;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtobufRejection {
  #[error("ProtobufDecode")]
  ProtobufDecodeError,
}

impl IntoResponse for ProtobufRejection {
  fn into_response(self) -> Response {
    match self {
      ProtobufRejection::ProtobufDecodeError => {
        (StatusCode::UNPROCESSABLE_ENTITY, "invalid input").into_response()
      }
    }
  }
}

/// A Protocol Buffer message extractor and response.
///
/// This can be used both as an extractor and as a response.
///
/// # As extractor
///
/// When used as an extractor, it can decode request bodies into some type that
/// implements [`prost::Message`]. The request will be rejected (and a [`ProtobufRejection`] will
/// be returned) if:
///
/// - The body couldn't be decoded into the target Protocol Buffer message type.
/// - Buffering the request body fails.
///
/// See [`ProtobufRejection`] for more details.
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(docsrs, doc(cfg(feature = "protobuf")))]
#[must_use]
pub struct Protobuf<T>(pub T);

impl<T, S> FromRequest<S> for Protobuf<T>
where
  T: Message + Default,
  S: Send + Sync,
{
  type Rejection = ProtobufRejection;

  async fn from_request(req: Request, _: &S) -> Result<Self, Self::Rejection> {
    let mut buf = req
      .into_limited_body()
      .collect()
      .await
      .map_err(|_| ProtobufRejection::ProtobufDecodeError)?
      .aggregate();

    match T::decode(&mut buf) {
      Ok(value) => Ok(Protobuf(value)),
      Err(_err) => Err(ProtobufRejection::ProtobufDecodeError),
    }
  }
}

// __impl_deref!(Protobuf);

impl<T> From<T> for Protobuf<T> {
  fn from(inner: T) -> Self {
    Self(inner)
  }
}

impl<T> IntoResponse for Protobuf<T>
where
  T: Message + Default,
{
  fn into_response(self) -> Response {
    let mut buf = BytesMut::with_capacity(self.0.encoded_len());
    match &self.0.encode(&mut buf) {
      Ok(()) => buf.into_response(),
      Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
  }
}

#[cfg(test)]
mod tests {
  use axum::body::Body;
  use axum::http::{Request, StatusCode};
  use axum::routing::{Router, post};
  use tower::ServiceExt;

  use super::*;
  use crate::config::proto::Config;

  #[tokio::test]
  async fn test_protobuf_deserialization() {
    async fn handler(Protobuf(config): Protobuf<Config>) -> String {
      return config.to_text().unwrap();
    }

    async fn oneshot(req: Request<Body>) -> (StatusCode, Vec<u8>) {
      let router = Router::new().route("/", post(handler));
      let resp = router.oneshot(req).await.unwrap();
      let status = resp.status();
      let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();

      return (status, body);
    }

    let (status, _) = oneshot(
      Request::builder()
        .method("POST")
        .uri("/")
        .body(Body::from("NOT A PROTO"))
        .unwrap(),
    )
    .await;

    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status);

    let config = Config::default();
    let (status, body) = oneshot(
      Request::builder()
        .method("POST")
        .uri("/")
        .body(Body::from(config.encode_to_vec()))
        .unwrap(),
    )
    .await;

    assert_eq!(StatusCode::OK, status);
    assert_eq!(config.to_text().unwrap(), String::from_utf8_lossy(&body));

    let (status, _) = oneshot(
      Request::builder()
        .method("POST")
        .uri("/")
        .body(Body::from(config.to_text().unwrap()))
        .unwrap(),
    )
    .await;

    // The Protobuf extractor currently only supports binary proto. Should we support textproto
    // as well? DynamicMessage::parse_text_format(body)?.transcode_to::<T>()?.
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status);
  }
}
