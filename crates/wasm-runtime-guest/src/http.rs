use futures_util::future::LocalBoxFuture;
use http::{HeaderMap, HeaderValue, Method, Version};
use trailbase_wasm_common::HttpContextUser as User;
use wstd::http::body::{IncomingBody, IntoBody};
use wstd::http::server::{Finished, Responder};
use wstd::http::{Response, StatusCode};
use wstd::io::empty;

use crate::wit::exports::trailbase::runtime::init_endpoint::MethodType;

pub struct HttpError {
  pub status: wstd::http::StatusCode,
  pub message: Option<String>,
}

type HttpHandler = Box<
  dyn (Fn(
      Option<User>,
      wstd::http::Request<IncomingBody>,
      wstd::http::server::Responder,
    ) -> LocalBoxFuture<'static, Finished>)
    + Send
    + Sync,
>;

pub struct HttpRoute {
  pub method: Method,
  pub path: String,
  pub handler: HttpHandler,
}

impl HttpRoute {
  pub fn new<F, R, B>(method: Method, path: &str, f: F) -> Self
  where
    F: (AsyncFn(Request) -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: wstd::http::body::Body,
  {
    let f = std::sync::Arc::new(f);
    return Self {
      method,
      path: path.to_string(),
      handler: Box::new(
        move |user: Option<User>, req: wstd::http::Request<IncomingBody>, responder: Responder| {
          let f = f.clone();

          let (head, body) = req.into_parts();
          let Ok(url) = to_url(head.uri) else {
            return Box::pin(responder.respond(error_response(StatusCode::INTERNAL_SERVER_ERROR)));
          };

          let req = Request {
            head: Parts {
              method: head.method,
              uri: url,
              version: head.version,
              headers: head.headers,
              user,
            },
            body,
          };

          return Box::pin(async move {
            let resp = f(req).await.into_response();
            // println!("Got response");
            let finished = responder.respond(resp).await;
            // println!("responded");
            finished
          });
        },
      ),
    };
  }
}

#[derive(Clone, Debug)]
pub struct Parts {
  /// The request's method
  pub method: Method,

  /// The request's URI
  pub uri: url::Url,

  /// The request's version
  pub version: Version,

  /// The request's headers
  pub headers: HeaderMap<HeaderValue>,

  /// User metadata
  pub user: Option<User>,
}

#[derive(Debug)]
pub struct Request {
  head: Parts,
  body: IncomingBody,
}

impl Request {
  #[inline]
  pub fn body(&self) -> &IncomingBody {
    return &self.body;
  }

  #[inline]
  pub fn url(&self) -> &url::Url {
    return &self.head.uri;
  }

  #[inline]
  pub fn method(&self) -> &Method {
    return &self.head.method;
  }

  #[inline]
  pub fn version(&self) -> &Version {
    return &self.head.version;
  }

  #[inline]
  pub fn header(&self, key: &str) -> Option<&HeaderValue> {
    return self.head.headers.get(key);
  }

  #[inline]
  pub fn user(&self) -> Option<&User> {
    return self.head.user.as_ref();
  }
}

fn to_url(uri: http::Uri) -> Result<url::Url, url::ParseError> {
  let http::uri::Parts {
    scheme,
    authority,
    path_and_query,
    ..
  } = uri.into_parts();

  return match (scheme, authority, path_and_query) {
    (Some(s), Some(a), Some(p)) => url::Url::parse(&format!("{s}://{a}/{p}")),
    (_, _, Some(p)) => url::Url::parse(p.as_str()),
    _ => Err(url::ParseError::RelativeUrlWithCannotBeABaseBase),
  };
}

impl TryFrom<wstd::http::Request<IncomingBody>> for Request {
  type Error = url::ParseError;

  fn try_from(req: wstd::http::Request<IncomingBody>) -> Result<Self, Self::Error> {
    let (head, body) = req.into_parts();

    return Ok(Request {
      head: Parts {
        method: head.method,
        uri: to_url(head.uri)?,
        version: head.version,
        headers: head.headers,
        // FIXME:
        user: None,
      },
      body,
    });
  }
}

pub trait IntoResponse<B: wstd::http::body::Body> {
  fn into_response(self) -> http::Response<B>;
}

type BoundedBody = wstd::http::body::BoundedBody<Vec<u8>>;

impl IntoResponse<BoundedBody> for Vec<u8> {
  fn into_response(self) -> http::Response<BoundedBody> {
    return Response::builder().body(self.into_body()).unwrap();
  }
}

impl IntoResponse<BoundedBody> for &[u8] {
  fn into_response(self) -> http::Response<BoundedBody> {
    return Response::builder().body(self.into_body()).unwrap();
  }
}

impl IntoResponse<BoundedBody> for String {
  fn into_response(self) -> http::Response<BoundedBody> {
    return Response::builder().body(self.into_body()).unwrap();
  }
}

impl IntoResponse<BoundedBody> for &str {
  fn into_response(self) -> http::Response<BoundedBody> {
    return Response::builder().body(self.into_body()).unwrap();
  }
}

impl IntoResponse<BoundedBody> for () {
  fn into_response(self) -> http::Response<BoundedBody> {
    return Response::builder().body(b"".into_body()).unwrap();
  }
}

impl IntoResponse<BoundedBody> for Result<Vec<u8>, HttpError> {
  fn into_response(self) -> http::Response<BoundedBody> {
    return match self {
      Ok(body) => Response::builder().body(body.into_body()).unwrap(),
      Err(err) => Response::builder()
        .status(err.status)
        .body(err.message.unwrap_or_default().into_body())
        .unwrap(),
    };
  }
}

impl IntoResponse<BoundedBody> for Result<&[u8], HttpError> {
  fn into_response(self) -> http::Response<BoundedBody> {
    return match self {
      Ok(body) => Response::builder().body(body.into_body()).unwrap(),
      Err(err) => Response::builder()
        .status(err.status)
        .body(err.message.unwrap_or_default().into_body())
        .unwrap(),
    };
  }
}

impl IntoResponse<BoundedBody> for Result<String, HttpError> {
  fn into_response(self) -> http::Response<BoundedBody> {
    return match self {
      Ok(body) => Response::builder().body(body.into_body()).unwrap(),
      Err(err) => Response::builder()
        .status(err.status)
        .body(err.message.unwrap_or_default().into_body())
        .unwrap(),
    };
  }
}

impl IntoResponse<BoundedBody> for Result<(), HttpError> {
  fn into_response(self) -> http::Response<BoundedBody> {
    return match self {
      Ok(_) => Response::builder().body(b"".into_body()).unwrap(),
      Err(err) => Response::builder()
        .status(err.status)
        .body(err.message.unwrap_or_default().into_body())
        .unwrap(),
    };
  }
}

impl IntoResponse<BoundedBody> for Result<&str, HttpError> {
  fn into_response(self) -> http::Response<BoundedBody> {
    return match self {
      Ok(body) => Response::builder().body(body.into_body()).unwrap(),
      Err(err) => Response::builder()
        .status(err.status)
        .body(err.message.unwrap_or_default().into_body())
        .unwrap(),
    };
  }
}

impl From<Method> for MethodType {
  fn from(m: Method) -> MethodType {
    return match m {
      Method::GET => MethodType::Get,
      Method::POST => MethodType::Post,
      Method::HEAD => MethodType::Head,
      Method::OPTIONS => MethodType::Options,
      Method::PATCH => MethodType::Patch,
      Method::DELETE => MethodType::Delete,
      Method::PUT => MethodType::Put,
      Method::TRACE => MethodType::Trace,
      Method::CONNECT => MethodType::Connect,
      _ => unreachable!(""),
    };
  }
}

#[inline]
fn error_response(status: StatusCode) -> Response<wstd::io::Empty> {
  return Response::builder().status(status).body(empty()).unwrap();
}
