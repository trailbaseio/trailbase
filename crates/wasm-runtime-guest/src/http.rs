use futures_util::future::LocalBoxFuture;
use trailbase_wasm_common::HttpContextUser as User;
use wstd::http::server::{Finished, Responder};
use wstd::io::{Cursor, Empty, empty};

pub use http::{HeaderMap, HeaderValue, Method, Response, StatusCode, Version};

#[derive(Clone, Debug)]
pub struct HttpError {
  pub status: StatusCode,
  pub message: Option<String>,
}

impl HttpError {
  pub fn status(status: StatusCode) -> Self {
    return Self {
      status,
      message: None,
    };
  }

  pub fn message(status: StatusCode, message: impl std::string::ToString) -> Self {
    return Self {
      status,
      message: Some(message.to_string()),
    };
  }
}

type HttpHandler = Box<
  dyn (Fn(
    Option<User>,
    http::Request<wstd::http::body::IncomingBody>,
    wstd::http::server::Responder,
  ) -> LocalBoxFuture<'static, Finished>),
>;

pub struct HttpRoute {
  pub method: Method,
  pub path: String,
  pub handler: HttpHandler,
}

impl HttpRoute {
  pub fn new<F, R, B>(method: Method, path: impl std::string::ToString, f: F) -> Self
  where
    F: (AsyncFn(Request) -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: wstd::http::body::Body,
  {
    let f = std::rc::Rc::new(f);

    return Self {
      method,
      path: path.to_string(),
      handler: Box::new(
        move |user: Option<User>,
              req: http::Request<wstd::http::body::IncomingBody>,
              responder: Responder| {
          let (head, body) = req.into_parts();
          let Ok(url) = to_url(head.uri) else {
            return Box::pin(responder.respond(empty_error_response(StatusCode::BAD_REQUEST)));
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

          let f = f.clone();
          return Box::pin(async move {
            let response = responder.respond(f(req).await.into_response()).await;

            // TODO: Poll tasks.

            response
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
  body: wstd::http::body::IncomingBody,
}

impl Request {
  #[inline]
  pub fn body(&self) -> &wstd::http::body::IncomingBody {
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

/// An HTTP body with a known length
#[derive(Debug)]
pub struct BoundedBody<T>(Cursor<T>);

impl<T: AsRef<[u8]>> wstd::io::AsyncRead for BoundedBody<T> {
  async fn read(&mut self, buf: &mut [u8]) -> wstd::io::Result<usize> {
    self.0.read(buf).await
  }
}
impl<T: AsRef<[u8]>> wstd::http::body::Body for BoundedBody<T> {
  fn len(&self) -> Option<usize> {
    Some(self.0.get_ref().as_ref().len())
  }
}

/// Conversion into a `Body`.
///
/// NOTE: We have our own trait over wstd::http::body::IntoBody to avoid possible future conflicts
/// when implementing IntoResponse for Result<B: IntoBody, HttpError>.
pub trait IntoBody {
  /// What type of `Body` are we turning this into?
  type IntoBody: wstd::http::body::Body;
  /// Convert into `Body`.
  fn into_body(self) -> Self::IntoBody;
}

impl IntoBody for () {
  type IntoBody = wstd::io::Empty;

  fn into_body(self) -> Self::IntoBody {
    return wstd::io::empty();
  }
}

impl IntoBody for String {
  type IntoBody = BoundedBody<Vec<u8>>;
  fn into_body(self) -> Self::IntoBody {
    BoundedBody(Cursor::new(self.into_bytes()))
  }
}

impl IntoBody for &str {
  type IntoBody = BoundedBody<Vec<u8>>;
  fn into_body(self) -> Self::IntoBody {
    BoundedBody(Cursor::new(self.to_owned().into_bytes()))
  }
}

impl IntoBody for Vec<u8> {
  type IntoBody = BoundedBody<Vec<u8>>;
  fn into_body(self) -> Self::IntoBody {
    BoundedBody(Cursor::new(self))
  }
}

impl IntoBody for &[u8] {
  type IntoBody = BoundedBody<Vec<u8>>;
  fn into_body(self) -> Self::IntoBody {
    BoundedBody(Cursor::new(self.to_owned()))
  }
}

pub trait IntoResponse<B> {
  fn into_response(self) -> http::Response<B>;
}

impl<B: IntoBody> IntoResponse<B::IntoBody> for B {
  fn into_response(self) -> http::Response<B::IntoBody> {
    return Response::new(self.into_body());
  }
}

impl<B: IntoBody<IntoBody = BoundedBody<Vec<u8>>>> IntoResponse<BoundedBody<Vec<u8>>>
  for Result<B, HttpError>
{
  fn into_response(self) -> http::Response<BoundedBody<Vec<u8>>> {
    return match self {
      Ok(body) => Response::new(body.into_body()),
      Err(err) => build_response(err.status, err.message.unwrap_or_default().into_body()),
    };
  }
}

impl IntoResponse<BoundedBody<Vec<u8>>> for Result<(), HttpError> {
  fn into_response(self) -> http::Response<BoundedBody<Vec<u8>>> {
    return match self {
      Ok(_) => Response::new("".into_body()),
      Err(err) => build_response(err.status, err.message.unwrap_or_default().into_body()),
    };
  }
}

#[derive(Debug, Clone, Copy, Default)]
#[must_use]
pub struct Json<T>(pub T);

impl<T> IntoResponse<BoundedBody<Vec<u8>>> for Json<T>
where
  T: serde::Serialize,
{
  fn into_response(self) -> http::Response<BoundedBody<Vec<u8>>> {
    return build_json_response(StatusCode::OK, self.0);
  }
}

impl<T> IntoResponse<BoundedBody<Vec<u8>>> for std::result::Result<Json<T>, HttpError>
where
  T: serde::Serialize,
{
  fn into_response(self) -> http::Response<BoundedBody<Vec<u8>>> {
    return match self {
      Ok(json) => {
        return build_json_response(StatusCode::OK, json.0);
      }
      Err(err) => build_response(err.status, err.message.unwrap_or_default().into_body()),
    };
  }
}

pub(crate) fn empty_error_response(status: StatusCode) -> Response<Empty> {
  let mut response = Response::new(empty());
  *response.status_mut() = status;
  return response;
}

fn internal_error_response() -> Response<BoundedBody<Vec<u8>>> {
  return build_response(StatusCode::INTERNAL_SERVER_ERROR, "".into_body());
}

#[inline]
fn build_response(
  status: StatusCode,
  body: BoundedBody<Vec<u8>>,
) -> Response<BoundedBody<Vec<u8>>> {
  let mut response = Response::new(body);
  *response.status_mut() = status;
  return response;
}

#[inline]
fn build_json_response<T: serde::Serialize>(
  status: StatusCode,
  value: T,
) -> Response<BoundedBody<Vec<u8>>> {
  let Ok(bytes) = serde_json::to_vec(&value) else {
    return internal_error_response();
  };

  let mut response = build_response(status, bytes.into_body());
  response.headers_mut().insert(
    http::header::CONTENT_TYPE,
    HeaderValue::from_static("application/json"),
  );

  return response;
}
