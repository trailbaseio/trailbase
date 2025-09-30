#![forbid(unsafe_code, clippy::unwrap_used)]
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
use wstd::http::Request;
use wstd::http::body::IncomingBody;
use wstd::http::server::{Finished, Responder};

use crate::http::{HttpRoute, Method, StatusCode, empty_error_response};
use crate::job::Job;

pub use crate::wit::exports::trailbase::runtime::init_endpoint::{InitArguments, InitResult};

// Needed for export macro
pub use static_assertions::assert_impl_all;
pub use wstd::wasip2 as __wasi;

#[macro_export]
macro_rules! export {
    ($impl:ident) => {
        ::trailbase_wasm::assert_impl_all!($impl: ::trailbase_wasm::Guest);
        // Register InitEndpoint.
        ::trailbase_wasm::wit::export!($impl);
        // Register Incoming HTTP handler.
        type _HttpHandlerIdent = ::trailbase_wasm::HttpIncomingHandler<$impl>;
        ::trailbase_wasm::__wasi::http::proxy::export!(
            _HttpHandlerIdent with_types_in ::trailbase_wasm::__wasi);
    };
}

#[derive(Debug)]
pub struct Args {
  pub version: Option<String>,
}

pub trait Guest {
  fn init(_: Args) {}

  fn http_handlers() -> Vec<HttpRoute> {
    return vec![];
  }

  fn job_handlers() -> Vec<Job> {
    return vec![];
  }
}

impl<T: Guest> crate::wit::exports::trailbase::runtime::init_endpoint::Guest for T {
  fn init(args: InitArguments) -> InitResult {
    T::init(Args {
      version: args.version,
    });

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
        .respond(empty_error_response(StatusCode::INTERNAL_SERVER_ERROR))
        .await;
    };

    log::debug!("WASM guest received HTTP request {path}: {context:?}");

    match context.kind {
      HttpContextKind::Http => {
        if let Some(HttpRoute { handler, .. }) = T::http_handlers()
          .into_iter()
          .find(|route| route.method == method && route.path == context.registered_path)
        {
          return handler(context, request, responder).await;
        }
      }
      HttpContextKind::Job => {
        if let Some(Job { handler, .. }) = T::job_handlers()
          .into_iter()
          .find(|config| method == Method::GET && config.name == context.registered_path)
        {
          return handler(responder).await;
        }
      }
    }

    return responder
      .respond(empty_error_response(StatusCode::NOT_FOUND))
      .await;
  }
}

impl<T: Guest> ::wstd::wasip2::exports::http::incoming_handler::Guest for HttpIncomingHandler<T> {
  fn handle(
    request: ::wstd::wasip2::http::types::IncomingRequest,
    response_out: ::wstd::wasip2::http::types::ResponseOutparam,
  ) {
    let responder = Responder::new(response_out);

    let _finished: Finished = match ::wstd::http::request::try_from_incoming(request) {
      Ok(request) => ::wstd::runtime::block_on(async { Self::handle(request, responder).await }),
      Err(err) => responder.fail(err),
    };
  }
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
