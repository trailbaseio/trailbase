//#![forbid(unsafe_code, clippy::unwrap_used)]
#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod wit {
  wit_bindgen::generate!({
      world: "trailbase:runtime/trailbase",
      path: [
          // Order-sensitive: will import *.wit from the folder.
          "wit/deps-0.2.6/random",
          "wit/deps-0.2.6/io",
          "wit/deps-0.2.6/clocks",
          "wit/deps-0.2.6/filesystem",
          "wit/deps-0.2.6/sockets",
          "wit/deps-0.2.6/cli",
          "wit/deps-0.2.6/http",
          "wit/keyvalue-0.2.0-draft",
          // Ours:
          "wit/trailbase.wit",
      ],
      pub_export_macro: true,
      default_bindings_module: "trailbase_wasm::wit",
      // additional_derives: [PartialEq, Eq, Hash, Clone],
      generate_all,
  });
}

pub mod db;
pub mod fetch;
pub mod fs;
pub mod http;
pub mod job;
pub mod kv;
pub mod time;

use trailbase_wasm_common::{HttpContext, HttpContextKind};
use wstd::http::body::IncomingBody;
use wstd::http::server::{Finished, Responder};
use wstd::http::{Request, Response};
use wstd::io::empty;

use crate::http::{HttpRoute, Method, StatusCode};
use crate::job::Job;

// Needed for export macro
pub use static_assertions::assert_impl_all;
pub use wstd::wasi;

pub use crate::wit::exports::trailbase::runtime::init_endpoint::InitResult;
pub use crate::wit::trailbase::runtime::host_endpoint::thread_id;

#[macro_export]
macro_rules! export {
    ($impl:ident) => {
        ::trailbase_wasm::assert_impl_all!($impl: ::trailbase_wasm::Guest);
        // Register InitEndpoint.
        ::trailbase_wasm::wit::export!($impl);
        // Register Incoming HTTP handler.
        type _HttpHandlerIdent = ::trailbase_wasm::HttpIncomingHandler<$impl>;
        ::trailbase_wasm::wasi::http::proxy::export!(
            _HttpHandlerIdent with_types_in ::trailbase_wasm::wasi);
    };
}

pub trait Guest {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![];
  }

  fn job_handlers() -> Vec<Job> {
    return vec![];
  }
}

impl<T: Guest> crate::wit::exports::trailbase::runtime::init_endpoint::Guest for T {
  fn init() -> InitResult {
    return InitResult {
      http_handlers: T::http_handlers()
        .into_iter()
        .map(|route| (to_method_type(route.method), route.path))
        .collect(),
      job_handlers: T::job_handlers()
        .into_iter()
        .map(|config| (config.name, config.spec))
        .collect(),
    };
  }
}

pub struct HttpIncomingHandler<T: Guest> {
  phantom: std::marker::PhantomData<T>,
}

impl<T: Guest> HttpIncomingHandler<T> {
  async fn handle(request: Request<IncomingBody>, responder: Responder) -> Finished {
    let path = request.uri().path();
    let method = request.method();

    let Some(context) = request
      .headers()
      .get("__context")
      .and_then(|h| serde_json::from_slice::<HttpContext>(h.as_bytes()).ok())
    else {
      return responder
        .respond(error_response(StatusCode::INTERNAL_SERVER_ERROR))
        .await;
    };

    log::debug!("WASM guest received HTTP request {path}: {context:?}");

    match context.kind {
      HttpContextKind::Http => {
        if let Some(HttpRoute { handler, .. }) = T::http_handlers()
          .into_iter()
          .find(|route| route.method == method && route.path == context.registered_path)
        {
          return handler(context.user, request, responder).await;
        }
      }
      HttpContextKind::Job => {
        if let Some(Job { handler, .. }) = T::job_handlers()
          .into_iter()
          .find(|config| method == Method::GET && config.name == context.registered_path)
        {
          if let Err(err) = handler().await {
            return responder.respond(error_response(err.status)).await;
          }

          return responder
            .respond(Response::builder().body(empty()).unwrap())
            .await;
        }
      }
    }

    return responder
      .respond(error_response(StatusCode::NOT_FOUND))
      .await;
  }
}

impl<T: Guest> ::wstd::wasi::exports::http::incoming_handler::Guest for HttpIncomingHandler<T> {
  fn handle(
    request: ::wstd::wasi::http::types::IncomingRequest,
    response_out: ::wstd::wasi::http::types::ResponseOutparam,
  ) {
    let responder = Responder::new(response_out);

    let _finished: Finished = match ::wstd::http::request::try_from_incoming(request) {
      Ok(request) => ::wstd::runtime::block_on(async { Self::handle(request, responder).await }),
      Err(err) => responder.fail(err),
    };
  }
}

#[inline]
fn error_response(status: StatusCode) -> Response<wstd::io::Empty> {
  return Response::builder().status(status).body(empty()).unwrap();
}

fn to_method_type(m: Method) -> crate::wit::exports::trailbase::runtime::init_endpoint::MethodType {
  use crate::wit::exports::trailbase::runtime::init_endpoint::MethodType;

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
    _ => panic!("extension"),
  };
}
