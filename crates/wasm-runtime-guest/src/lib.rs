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
use trailbase_wasm_common::{SqliteRequest, SqliteResponse};
use wstd::http::body::{BodyForthcoming, IncomingBody, IntoBody};
use wstd::http::server::{Finished, Responder};
use wstd::http::{Client, Request, Response, StatusCode};
use wstd::io::{AsyncWrite, empty};

pub use crate::wit::exports::trailbase::runtime::init_endpoint::{Guest, InitResult, MethodType};
pub use crate::wit::trailbase::runtime::host_endpoint::thread_id;
pub use wstd::http::Method;

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

pub trait Init {
  fn http_handlers() -> Vec<(Method, &'static str, Handler)>;
  fn job_handlers() -> Vec<(String, String)> {
    return vec![];
  }
}
impl<T: Init> Guest for T {
  fn init() -> InitResult {
    let http_handlers: Vec<(MethodType, String)> = T::http_handlers()
      .into_iter()
      .map(|(m, p, _)| (m.into(), p.to_string()))
      .collect();

    return InitResult {
      http_handlers,
      job_handlers: T::job_handlers(),
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
      // FIXME:
      Method::CONNECT => MethodType::Trace,
      _ => unreachable!(""),
    };
  }
}

pub struct HttpError {
  pub status: wstd::http::StatusCode,
  pub message: Option<String>,
}

pub type Handler = Box<
  dyn (Fn(Request<IncomingBody>) -> LocalBoxFuture<'static, Result<Vec<u8>, HttpError>>)
    + Send
    + Sync,
>;

pub fn to_handler(
  f: impl (AsyncFn(Request<IncomingBody>) -> Result<Vec<u8>, HttpError>) + Send + Sync + 'static,
) -> Handler {
  let f = std::sync::Arc::new(f);
  return Box::new(move |req: Request<IncomingBody>| {
    let f = f.clone();
    Box::pin(async move { f(req).await })
  });
}

pub struct HttpIncomingHandler<T: Init> {
  phantom: std::marker::PhantomData<T>,
}

impl<T: Init> HttpIncomingHandler<T> {
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
        let handlers = T::http_handlers();

        if let Some((_, _, h)) = handlers.iter().find(|(m, p, _)| method == m && *p == path) {
          match h(request).await {
            Ok(response) => {
              return write_all(responder, &response).await;
            }
            Err(err) => {
              let response = Response::builder()
                .status(err.status)
                .body(empty())
                .unwrap();
              return responder.respond(response).await;
            }
          }
        }

        let response = Response::builder()
          .status(StatusCode::NOT_FOUND)
          .body(empty())
          .unwrap();
        responder.respond(response).await
      }
    };
  }
}

impl<T: Init> ::wstd::wasi::exports::http::incoming_handler::Guest for HttpIncomingHandler<T> {
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

// ::wstd::wasi::http::proxy::export!(TheServer with_types_in ::wstd::wasi);

async fn write_all(responder: Responder, buf: &[u8]) -> Finished {
  let mut body = responder.start_response(Response::new(BodyForthcoming));
  let result = body.write_all(buf).await;
  Finished::finish(body, result, None)
}
