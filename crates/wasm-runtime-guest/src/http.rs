use futures_util::future::LocalBoxFuture;
use serde::de::DeserializeOwned;
use trailbase_wasm_common::HttpContext;
use wstd::http::server::Responder;

pub use http::{HeaderMap, HeaderValue, Method, StatusCode, Version, header};
pub use trailbase_wasm_common::HttpContextUser as User;

pub type Response<T = wstd::http::Body> = http::Response<T>;

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
    http::Request<wstd::http::Body>,
    wstd::http::server::Responder,
  ) -> LocalBoxFuture<'static, Result<(), anyhow::Error>>,
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
    B: Into<wstd::http::Body>,
  {
    return Self {
      method,
      path: path.to_string(),
      handler: Box::new(
        move |context: HttpContext, req: http::Request<wstd::http::Body>, responder: Responder| {
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

            return response;
          });
        },
      ),
    };
  }

  /// Wrap route to be rejected for all non-admin users.
  ///
  /// NOTE: We're using a builder pattern on an HttpRoute instead of an HttpRouteBuilder, this may
  /// be surprising.
  pub fn require_admin(self) -> HttpRoute {
    let Self {
      method,
      path,
      handler: original,
    } = self;

    return HttpRoute {
      method,
      path,
      // Wraps the handler in an access check to build a new route.
      handler: Box::new(
        move |context: HttpContext, req: http::Request<wstd::http::Body>, responder: Responder| {
          Box::pin(async move {
            if let Err(err) = crate::auth::require_admin_impl(
              context.user.as_ref(),
              req.method(),
              req.headers().get(crate::auth::CSRF_HEADER),
            )
            .await
            {
              return responder.respond(err.into()).await;
            }

            return original(context, req, responder).await;
          })
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
    B: Into<wstd::http::Body>,
  {
    return HttpRoute::new(Method::GET, path, f);
  }

  pub fn post<F, R, B>(path: impl std::string::ToString, f: F) -> HttpRoute
  where
    F: (AsyncFn(Request) -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: Into<wstd::http::Body>,
  {
    return HttpRoute::new(Method::POST, path, f);
  }

  pub fn patch<F, R, B>(path: impl std::string::ToString, f: F) -> HttpRoute
  where
    F: (AsyncFn(Request) -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: Into<wstd::http::Body>,
  {
    return HttpRoute::new(Method::PATCH, path, f);
  }

  pub fn delete<F, R, B>(path: impl std::string::ToString, f: F) -> HttpRoute
  where
    F: (AsyncFn(Request) -> R) + Send + Sync + 'static,
    R: IntoResponse<B>,
    B: Into<wstd::http::Body>,
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
  body: wstd::http::Body,
}

impl Request {
  #[inline]
  pub fn body(&mut self) -> &mut wstd::http::Body {
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

/// Conversion into a `Body`.
pub trait IntoBody {
  fn into_body(self) -> wstd::http::Body;
}

impl IntoBody for () {
  fn into_body(self) -> wstd::http::Body {
    return self.into();
  }
}

impl IntoBody for String {
  fn into_body(self) -> wstd::http::Body {
    return self.into();
  }
}

impl IntoBody for &str {
  fn into_body(self) -> wstd::http::Body {
    return self.into();
  }
}

impl IntoBody for Vec<u8> {
  fn into_body(self) -> wstd::http::Body {
    return self.into();
  }
}

impl IntoBody for bytes::Bytes {
  fn into_body(self) -> wstd::http::Body {
    return self.into();
  }
}

impl IntoBody for &[u8] {
  fn into_body(self) -> wstd::http::Body {
    return self.into();
  }
}

pub trait IntoResponse<B> {
  fn into_response(self) -> http::Response<B>;
}

impl<B: Into<wstd::http::Body>> IntoResponse<B> for Response<B> {
  fn into_response(self) -> http::Response<B> {
    return self;
  }
}

impl<B: Into<wstd::http::Body>, Err: IntoResponse<B>> IntoResponse<B> for Result<Response<B>, Err> {
  fn into_response(self) -> http::Response<B> {
    return match self {
      Ok(resp) => resp,
      Err(err) => err.into_response(),
    };
  }
}

impl<B: IntoBody> IntoResponse<wstd::http::Body> for B {
  fn into_response(self) -> http::Response<wstd::http::Body> {
    return http::Response::new(self.into_body());
  }
}

impl<B: IntoBody> IntoResponse<wstd::http::Body> for Result<B, HttpError> {
  fn into_response(self) -> http::Response<wstd::http::Body> {
    return match self {
      Ok(body) => http::Response::new(body.into_body()),
      Err(err) => build_response(err.status, err.message.unwrap_or_default().into_body()),
    };
  }
}

impl IntoResponse<wstd::http::Body> for HttpError {
  fn into_response(self) -> http::Response<wstd::http::Body> {
    return build_response(self.status, self.message.unwrap_or_default().into_body());
  }
}

#[derive(Debug, Clone, Copy, Default)]
#[must_use]
pub struct Json<T>(pub T);

impl<T> IntoResponse<wstd::http::Body> for Json<T>
where
  T: serde::Serialize,
{
  fn into_response(self) -> http::Response<wstd::http::Body> {
    return build_json_response(StatusCode::OK, self.0);
  }
}

impl<T> IntoResponse<wstd::http::Body> for std::result::Result<Json<T>, HttpError>
where
  T: serde::Serialize,
{
  fn into_response(self) -> http::Response<wstd::http::Body> {
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

impl<T> IntoResponse<wstd::http::Body> for Html<T>
where
  T: IntoResponse<wstd::http::Body>,
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

impl IntoResponse<wstd::http::Body> for Redirect {
  fn into_response(self) -> http::Response<wstd::http::Body> {
    let mut response = http::Response::new(wstd::http::Body::empty());
    *response.status_mut() = self.status_code;
    response
      .headers_mut()
      .insert(http::header::LOCATION, self.location);
    return response;
  }
}

pub(crate) fn empty_error_response(status: StatusCode) -> http::Response<wstd::http::Body> {
  let mut response = http::Response::new(wstd::http::Body::empty());
  *response.status_mut() = status;
  return response;
}

fn internal_error_response() -> http::Response<wstd::http::Body> {
  return build_response(StatusCode::INTERNAL_SERVER_ERROR, "".into_body());
}

#[inline]
fn build_response(status: StatusCode, body: wstd::http::Body) -> http::Response<wstd::http::Body> {
  let mut response = http::Response::new(body);
  *response.status_mut() = status;
  return response;
}

#[inline]
fn build_json_response<T: serde::Serialize>(
  status: StatusCode,
  value: T,
) -> http::Response<wstd::http::Body> {
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
