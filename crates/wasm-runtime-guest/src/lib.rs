#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod wit {
  wit_bindgen::generate!({
      world: "trailbase:component/interfaces",
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
          "wit/trailbase/database",
          "wit/trailbase/component",
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

pub use crate::wit::exports::trailbase::component::init_endpoint::Arguments;

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

  fn sqlite_functions() -> Vec<String> {
    return vec![];
  }
}

impl<T: Guest> crate::wit::exports::trailbase::component::init_endpoint::Guest for T {
  fn init_http_handlers(
    args: Arguments,
  ) -> wit::exports::trailbase::component::init_endpoint::HttpHandlers {
    // QUESTION: Should we ensure that init is called only once?
    T::init(Args {
      version: args.version,
    });

    return wit::exports::trailbase::component::init_endpoint::HttpHandlers {
      handlers: T::http_handlers()
        .into_iter()
        .map(|route| (to_method_type(route.method), route.path))
        .collect(),
    };
  }

  fn init_job_handlers(
    args: Arguments,
  ) -> wit::exports::trailbase::component::init_endpoint::JobHandlers {
    T::init(Args {
      version: args.version,
    });

    return wit::exports::trailbase::component::init_endpoint::JobHandlers {
      handlers: T::job_handlers()
        .into_iter()
        .map(|config| (config.name, config.spec))
        .collect(),
    };
  }

  fn init_sqlite_functions(
    args: Arguments,
  ) -> wit::exports::trailbase::component::init_endpoint::SqliteFunctions {
    return wit::exports::trailbase::component::init_endpoint::SqliteFunctions {
      functions: T::sqlite_functions(),
    };
  }
}

impl<T: Guest> crate::wit::exports::trailbase::component::sqlite_function_endpoint::Guest for T {
  fn dispatch(
    args: crate::wit::exports::trailbase::component::sqlite_function_endpoint::Arguments,
  ) -> crate::wit::exports::trailbase::component::sqlite_function_endpoint::Response {
    return crate::wit::exports::trailbase::component::sqlite_function_endpoint::Response {
      response: "foo".to_string(),
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

fn to_method_type(
  m: Method,
) -> crate::wit::exports::trailbase::component::init_endpoint::HttpMethodType {
  use crate::wit::exports::trailbase::component::init_endpoint::HttpMethodType;

  return match m {
    Method::GET => HttpMethodType::Get,
    Method::POST => HttpMethodType::Post,
    Method::HEAD => HttpMethodType::Head,
    Method::OPTIONS => HttpMethodType::Options,
    Method::PATCH => HttpMethodType::Patch,
    Method::DELETE => HttpMethodType::Delete,
    Method::PUT => HttpMethodType::Put,
    Method::TRACE => HttpMethodType::Trace,
    Method::CONNECT => HttpMethodType::Connect,
    _ => panic!("extension"),
  };
}
