//#![forbid(unsafe_code, clippy::unwrap_used)]
#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod wit {
  wit_bindgen::generate!({
      world: "trailbase:runtime/trailbase",
      path: [
          // Order-sensitive: will import *.wit from the folder.
          "../wasm-runtime/wit/deps-0.2.6/random",
          "../wasm-runtime/wit/deps-0.2.6/io",
          "../wasm-runtime/wit/deps-0.2.6/clocks",
          "../wasm-runtime/wit/deps-0.2.6/filesystem",
          "../wasm-runtime/wit/deps-0.2.6/sockets",
          "../wasm-runtime/wit/deps-0.2.6/cli",
          "../wasm-runtime/wit/deps-0.2.6/http",
          // Ours:
          "../wasm-runtime/wit/trailbase.wit",
      ],
      pub_export_macro: true,
      default_bindings_module: "trailbase_wasm_guest::wit",
      // additional_derives: [PartialEq, Eq, Hash, Clone],
      generate_all,
  });
}

use futures_util::future::LocalBoxFuture;
use trailbase_wasm_common::{HttpContext, HttpContextKind, SqliteRequest, SqliteResponse};
use wstd::http::body::{BodyForthcoming, IncomingBody, IntoBody};
use wstd::http::server::{Finished, Responder};
use wstd::http::{Client, Request, Response, StatusCode};
use wstd::io::{AsyncWrite, empty};

use crate::wit::exports::trailbase::runtime::init_endpoint::MethodType;

pub use crate::wit::exports::trailbase::runtime::init_endpoint::InitResult;
pub use crate::wit::trailbase::runtime::host_endpoint::thread_id;
pub use static_assertions::assert_impl_all;
pub use wstd::{self, http::Method};

#[macro_export]
macro_rules! export {
    ($impl:ident) => {
        ::trailbase_wasm_guest::assert_impl_all!($impl: ::trailbase_wasm_guest::Guest);
        // Register InitEndpoint.
        ::trailbase_wasm_guest::wit::export!($impl);
        // Register Incoming HTTP handler.
        type _HttpHandlerIdent = ::trailbase_wasm_guest::HttpIncomingHandler<$impl>;
        ::trailbase_wasm_guest::wstd::wasi::http::proxy::export!(
            _HttpHandlerIdent with_types_in ::trailbase_wasm_guest::wstd::wasi);
    };
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Sqlite: {0}")]
  Sqlite(String),
}

pub type Rows = Vec<Vec<serde_json::Value>>;

pub async fn query(query: &str, params: Vec<serde_json::Value>) -> Result<Rows, Error> {
  let r = SqliteRequest {
    query: query.to_string(),
    params,
  };
  let bytes = serde_json::to_vec(&r).expect("serialization");

  let request = Request::builder()
    .uri("http://__sqlite/query")
    .method("POST")
    .body(bytes.into_body());

  let client = Client::new();
  let (_parts, mut body) = client
    .send(request.unwrap())
    .await
    .expect("foo")
    .into_parts();

  let bytes = body.bytes().await.expect("baz");
  return match serde_json::from_slice(&bytes) {
    Ok(SqliteResponse::Query { rows }) => Ok(rows),
    Ok(_) => Err(Error::Sqlite("Unexpected response type".to_string())),
    Err(err) => Err(Error::Sqlite(err.to_string())),
  };
}

pub trait Guest {
  fn http_handlers() -> Vec<(Method, &'static str, HttpHandler)> {
    return vec![];
  }

  fn job_handlers() -> Vec<(&'static str, &'static str, JobHandler)> {
    return vec![];
  }
}
impl<T: Guest> crate::wit::exports::trailbase::runtime::init_endpoint::Guest for T {
  fn init() -> InitResult {
    return InitResult {
      http_handlers: T::http_handlers()
        .into_iter()
        .map(|(m, p, _)| (m.into(), p.to_string()))
        .collect(),
      job_handlers: T::job_handlers()
        .into_iter()
        .map(|(name, spec, _)| (name.into(), spec.into()))
        .collect(),
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

pub struct HttpError {
  pub status: wstd::http::StatusCode,
  pub message: Option<String>,
}

pub type HttpHandler = Box<
  dyn (Fn(Request<IncomingBody>) -> LocalBoxFuture<'static, Result<Vec<u8>, HttpError>>)
    + Send
    + Sync,
>;

pub fn http_handler(
  f: impl (AsyncFn(Request<IncomingBody>) -> Result<Vec<u8>, HttpError>) + Send + Sync + 'static,
) -> HttpHandler {
  let f = std::sync::Arc::new(f);
  return Box::new(move |req: Request<IncomingBody>| {
    let f = f.clone();
    Box::pin(async move { f(req).await })
  });
}

pub type JobHandler =
  Box<dyn (Fn() -> LocalBoxFuture<'static, Result<(), HttpError>>) + Send + Sync>;

// NOTE: We use anyhow here specifically to allow guests to attach context.
pub fn job_handler(
  f: impl (AsyncFn() -> Result<(), anyhow::Error>) + Send + Sync + 'static,
) -> JobHandler {
  let f = std::sync::Arc::new(f);
  return Box::new(move || {
    let f = f.clone();
    Box::pin(async move {
      f().await.map_err(|err| HttpError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: Some(format!("{err}")),
      })
    })
  });
}

pub struct HttpIncomingHandler<T: Guest> {
  phantom: std::marker::PhantomData<T>,
}

impl<T: Guest> HttpIncomingHandler<T> {
  async fn handle(request: Request<IncomingBody>, responder: Responder) -> Finished {
    let path = request.uri().path();
    let method = request.method();

    return match path {
      "/query" => {
        let query = std::future::ready("SELECT COUNT(*) FROM TEST".to_string()).await;
        let rows = crate::query(&query, vec![]).await.unwrap();
        if rows[0][0] != serde_json::json!(1) {
          panic!("Expected one");
        }

        write_all(responder, format!("response: {rows:?}").as_bytes()).await
      }
      path => {
        let Some(context) = request
          .headers()
          .get("__context")
          .and_then(|h| serde_json::from_slice::<HttpContext>(h.as_bytes()).ok())
        else {
          return responder
            .respond(error_response(StatusCode::INTERNAL_SERVER_ERROR))
            .await;
        };

        println!("WASM guest received HTTP request {path}: {context:?}");

        match context.kind {
          HttpContextKind::Http => {
            if let Some((_, _, h)) = T::http_handlers()
              .into_iter()
              .find(|(m, p, _)| method == m && *p == context.registered_path)
            {
              match h(request).await {
                Ok(response) => {
                  return write_all(responder, &response).await;
                }
                Err(err) => {
                  return responder.respond(error_response(err.status)).await;
                }
              }
            }
          }
          HttpContextKind::Job => {
            if let Some((_, _, h)) = T::job_handlers()
              .into_iter()
              .find(|(m, p, _)| method == m && *p == context.registered_path)
            {
              if let Err(err) = h().await {
                return responder.respond(error_response(err.status)).await;
              }
              return write_all(responder, b"").await;
            }
          }
        }

        responder
          .respond(error_response(StatusCode::NOT_FOUND))
          .await
      }
    };
  }
}

impl<T: Guest> ::wstd::wasi::exports::http::incoming_handler::Guest for HttpIncomingHandler<T> {
  fn handle(
    request: ::wstd::wasi::http::types::IncomingRequest,
    response_out: ::wstd::wasi::http::types::ResponseOutparam,
  ) {
    let responder = ::wstd::http::server::Responder::new(response_out);

    let _finished: ::wstd::http::server::Finished =
      match ::wstd::http::request::try_from_incoming(request) {
        Ok(request) => ::wstd::runtime::block_on(async { Self::handle(request, responder).await }),
        Err(err) => responder.fail(err),
      };
  }
}

async fn write_all(responder: Responder, buf: &[u8]) -> Finished {
  let mut body = responder.start_response(Response::new(BodyForthcoming));
  let result = body.write_all(buf).await;
  Finished::finish(body, result, None)
}

fn error_response(status: StatusCode) -> Response<wstd::io::Empty> {
  return Response::builder().status(status).body(empty()).unwrap();
}
