use futures_util::future::LocalBoxFuture;
use serde::de::DeserializeOwned;
use trailbase_wasm_common::HttpContext;
use wstd::http::server::{Finished, Responder};
use wstd::io::{Cursor, Empty, empty};

pub use http::{HeaderMap, HeaderValue, Method, StatusCode, Version, header};
pub use trailbase_wasm_common::HttpContextUser as User;

pub type Response<T = BoundedBody<Vec<u8>>> = http::Response<T>;

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

impl From<HttpError> for Response {
  fn from(value: HttpError) -> Self {
    return value.into_response();
  }
}

type HttpHandler = Box<
  dyn FnOnce(
    HttpContext,
    http::Request<wstd::http::body::IncomingBody>,
    wstd::http::server::Responder,
  ) -> LocalBoxFuture<'static, Finished>,
>;

pub struct HttpRoute {
  pub method: Method,
  pub path: String,
  pub handler: HttpHandler,
}

impl HttpRoute {
  pub fn new<F, R, B>(method: Method, path: impl std::string::ToString, f: F) -> Self
  where
    // NOTE: Send + Sync aren't strictly needed. We could also accept AsyncFnOnce, however let's
    // start more constraint and see where it takes us.
    F: (AsyncFn(Request) -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: wstd::http::body::Body,
  {
    return Self {
      method,
      path: path.to_string(),
      handler: Box::new(
        move |context: HttpContext,
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
              user: context.user,
              path_params: context.path_params,
            },
            body,
          };

          return Box::pin(async move {
            #[allow(clippy::let_and_return)]
            let response = responder.respond(f(req).await.into_response()).await;

            // TODO: Poll tasks.

            response
          });
        },
      ),
    };
  }
}

pub mod routing {
  use super::{HttpRoute, IntoResponse, Method, Request};

  pub fn get<F, R, B>(path: impl std::string::ToString, f: F) -> HttpRoute
  where
    F: (AsyncFn(Request) -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: wstd::http::body::Body,
  {
    return HttpRoute::new(Method::GET, path, f);
  }

  pub fn post<F, R, B>(path: impl std::string::ToString, f: F) -> HttpRoute
  where
    F: (AsyncFn(Request) -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: wstd::http::body::Body,
  {
    return HttpRoute::new(Method::POST, path, f);
  }

  pub fn patch<F, R, B>(path: impl std::string::ToString, f: F) -> HttpRoute
  where
    F: (AsyncFn(Request) -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: wstd::http::body::Body,
  {
    return HttpRoute::new(Method::PATCH, path, f);
  }

  pub fn delete<F, R, B>(path: impl std::string::ToString, f: F) -> HttpRoute
  where
    F: (AsyncFn(Request) -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: wstd::http::body::Body,
  {
    return HttpRoute::new(Method::DELETE, path, f);
  }
}

// Disallow external construction.
#[non_exhaustive]
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

  /// Path params, e.g. /test/{param}/.
  pub path_params: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct Request {
  head: Parts,
  body: wstd::http::body::IncomingBody,
}

impl Request {
  #[inline]
  pub fn body(&mut self) -> &mut wstd::http::body::IncomingBody {
    return &mut self.body;
  }

  #[inline]
  pub fn url(&self) -> &url::Url {
    return &self.head.uri;
  }

  pub fn query_parse<T: DeserializeOwned>(&self) -> Result<T, HttpError> {
    let query = self.head.uri.query().unwrap_or_default();
    let deserializer =
      serde_urlencoded::Deserializer::new(url::form_urlencoded::parse(query.as_bytes()));
    return serde_path_to_error::deserialize(deserializer)
      .map_err(|err| HttpError::message(StatusCode::BAD_REQUEST, err));
  }

  // pub fn query_pairs(&self) -> url::form_urlencoded::Parse<'_> {
  //   self.head.uri.query_pairs()
  // }

  pub fn query_param(&self, param: &str) -> Option<String> {
    return self
      .head
      .uri
      .query_pairs()
      .find(|(p, _v)| p == param)
      .map(|(_p, v)| v.to_string());
  }

  pub fn path_param(&self, param: &str) -> Option<&str> {
    return self
      .head
      .path_params
      .iter()
      .find(|(p, _v)| p == param)
      .map(|(_p, v)| v.as_str());
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
#[derive(Debug, Default)]
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

impl<B: wstd::http::body::Body> IntoResponse<B> for Response<B> {
  fn into_response(self) -> http::Response<B> {
    return self;
  }
}

impl<B: wstd::http::body::Body, Err: IntoResponse<B>> IntoResponse<B> for Result<Response<B>, Err> {
  fn into_response(self) -> http::Response<B> {
    return match self {
      Ok(resp) => resp,
      Err(err) => err.into_response(),
    };
  }
}

impl<B: IntoBody> IntoResponse<B::IntoBody> for B {
  fn into_response(self) -> http::Response<B::IntoBody> {
    return http::Response::new(self.into_body());
  }
}

impl<B: IntoBody<IntoBody = BoundedBody<Vec<u8>>>> IntoResponse<BoundedBody<Vec<u8>>>
  for Result<B, HttpError>
{
  fn into_response(self) -> http::Response<BoundedBody<Vec<u8>>> {
    return match self {
      Ok(body) => http::Response::new(body.into_body()),
      Err(err) => build_response(err.status, err.message.unwrap_or_default().into_body()),
    };
  }
}

impl IntoResponse<BoundedBody<Vec<u8>>> for HttpError {
  fn into_response(self) -> http::Response<BoundedBody<Vec<u8>>> {
    return build_response(self.status, self.message.unwrap_or_default().into_body());
  }
}

impl IntoResponse<BoundedBody<Vec<u8>>> for Result<(), HttpError> {
  fn into_response(self) -> http::Response<BoundedBody<Vec<u8>>> {
    return match self {
      Ok(_) => http::Response::new("".into_body()),
      Err(err) => err.into_response(),
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

/// An HTML response.
///
/// Will automatically get `Content-Type: text/html`.
#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct Html<T>(pub T);

impl<T> IntoResponse<BoundedBody<Vec<u8>>> for Html<T>
where
  T: IntoResponse<BoundedBody<Vec<u8>>>,
{
  fn into_response(self) -> Response {
    let mut r = self.0.into_response();
    r.headers_mut().insert(
      http::header::CONTENT_TYPE,
      http::HeaderValue::from_static("text/html; charset=utf-8"),
    );
    return r;
  }
}

#[derive(Debug, Clone)]
#[must_use = "needs to be returned from a handler or otherwise turned into a Response to be useful"]
pub struct Redirect {
  status_code: StatusCode,
  location: http::header::HeaderValue,
}

impl Redirect {
  pub fn to(uri: &str) -> Self {
    Self::with_status_code(StatusCode::SEE_OTHER, uri)
  }

  pub fn temporary(uri: &str) -> Self {
    Self::with_status_code(StatusCode::TEMPORARY_REDIRECT, uri)
  }

  pub fn permanent(uri: &str) -> Self {
    Self::with_status_code(StatusCode::PERMANENT_REDIRECT, uri)
  }

  fn with_status_code(status_code: StatusCode, uri: &str) -> Self {
    assert!(
      status_code.is_redirection(),
      "not a redirection status code"
    );

    Self {
      status_code,
      location: HeaderValue::try_from(uri).expect("URI isn't a valid header value"),
    }
  }
}

impl<B: wstd::http::body::Body + Default> IntoResponse<B> for Redirect {
  fn into_response(self) -> http::Response<B> {
    let mut response = http::Response::<B>::default();
    *response.status_mut() = self.status_code;
    response
      .headers_mut()
      .insert(http::header::LOCATION, self.location);
    return response;
  }
}

pub(crate) fn empty_error_response(status: StatusCode) -> http::Response<Empty> {
  let mut response = http::Response::new(empty());
  *response.status_mut() = status;
  return response;
}

fn internal_error_response() -> http::Response<BoundedBody<Vec<u8>>> {
  return build_response(StatusCode::INTERNAL_SERVER_ERROR, "".into_body());
}

#[inline]
fn build_response(
  status: StatusCode,
  body: BoundedBody<Vec<u8>>,
) -> http::Response<BoundedBody<Vec<u8>>> {
  let mut response = http::Response::new(body);
  *response.status_mut() = status;
  return response;
}

#[inline]
fn build_json_response<T: serde::Serialize>(
  status: StatusCode,
  value: T,
) -> http::Response<BoundedBody<Vec<u8>>> {
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
