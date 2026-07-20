#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod wit {
  wit_bindgen::generate!({
      world: "trailbase:component/interfaces",
      path: [
          // Order-sensitive: will import *.wit from the folder.
          "wit/wasi-0.2.12/random",
          "wit/wasi-0.2.12/io",
          "wit/wasi-0.2.12/clocks",
          "wit/wasi-0.2.12/filesystem",
          "wit/wasi-0.2.12/sockets",
          "wit/wasi-0.2.12/cli",
          "wit/wasi-0.2.12/http",
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

pub mod auth;
pub mod db;
pub mod fetch;
pub mod fs;
pub mod http;
pub mod job;
pub mod kv;
pub mod time;

use std::sync::OnceLock;
use trailbase_wasm_common::{HttpContext, HttpContextKind};
use wstd::http::Request;
use wstd::http::body::IncomingBody;
use wstd::http::server::{Finished, Responder};

use crate::http::{HttpRoute, Method, StatusCode, empty_error_response};
use crate::job::Job;

// Needed for export macro
pub use static_assertions::assert_impl_all;
pub use wstd::wasip2 as __wasi;

pub mod sqlite {
  pub use crate::wit::exports::trailbase::component::sqlite_function_endpoint::{Error, Value};
  pub use trailbase_wasm_common::manifest::SqliteFunctionFlag;
}

pub mod rand {
  pub use wstd::rand::{get_insecure_random_bytes, get_random_bytes};
}

// Re-export AdminModule so guest crates can use it without depending on
// trailbase-wasm-common directly.
pub use trailbase_wasm_common::manifest::AdminModule;

#[macro_export]
macro_rules! export {
    ($impl:ident) => {
        ::trailbase_wasm::assert_impl_all!($impl: ::trailbase_wasm::Guest);
        // Register InitEndpoint.
        type _TrailbaseHandlerIdent = ::trailbase_wasm::TrailbaseHandler<$impl>;
        ::trailbase_wasm::wit::export!(_TrailbaseHandlerIdent);
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

type SqliteFunctionHandler =
  Box<dyn Fn(Vec<sqlite::Value>) -> Result<sqlite::Value, sqlite::Error> + Send + Sync>;

pub struct SqliteFunction {
  name: String,
  num_args: u32,
  flags: Vec<sqlite::SqliteFunctionFlag>,
  handler: SqliteFunctionHandler,
}

impl SqliteFunction {
  pub fn new<const N: usize>(
    name: impl std::string::ToString,
    f: impl Fn([sqlite::Value; N]) -> Result<sqlite::Value, sqlite::Error> + Send + Sync + 'static,
    flags: &[sqlite::SqliteFunctionFlag],
  ) -> Self {
    return Self {
      name: name.to_string(),
      num_args: N as u32,
      flags: flags.into(),
      handler: Box::new(move |args| {
        return f(args.try_into().expect("wrong number of arguments"));
      }),
    };
  }
}

pub trait Guest {
  fn init(_: Args) {}

  fn http_handlers() -> Vec<HttpRoute> {
    return vec![];
  }

  fn job_handlers() -> Vec<Job> {
    return vec![];
  }

  fn sqlite_scalar_functions() -> Vec<SqliteFunction> {
    return vec![];
  }

  fn admin_module() -> Option<AdminModule> {
    return None;
  }
}

pub struct TrailbaseHandler<T: Guest> {
  phantom: std::marker::PhantomData<T>,
}

impl<T: Guest> TrailbaseHandler<T> {
  fn call_init_once(args: Args) -> &'static () {
    static S: OnceLock<()> = OnceLock::new();
    return S.get_or_init(|| T::init(args));
  }

  fn get_sqlite_scalar_functions() -> &'static [SqliteFunction] {
    // NOTE: This assumes that there's only one `T`, since static are shared across generics.
    static FUNCS: OnceLock<Vec<SqliteFunction>> = OnceLock::new();
    return FUNCS.get_or_init(T::sqlite_scalar_functions);
  }
}

impl<T: Guest> crate::wit::exports::trailbase::component::init_endpoint::Guest
  for TrailbaseHandler<T>
{
  fn get_manifest(args: String) -> Result<String, String> {
    use trailbase_wasm_common::manifest::{
      AdminModule, HttpRoute, InitArguments, InitManifest, Job, SqliteFunction,
      SqliteScalarFunction,
    };

    let args: InitArguments = serde_json::from_str(&args).map_err(|err| err.to_string())?;

    Self::call_init_once(Args {
      version: args.version,
    });

    let http_handlers: Option<Vec<_>> = if args
      .subsystems
      .as_ref()
      .is_none_or(|c| c.contains(&trailbase_wasm_common::manifest::Subsystem::Http))
    {
      let handlers = T::http_handlers();
      if handlers.is_empty() {
        None
      } else {
        Some(
          handlers
            .into_iter()
            .map(|route| HttpRoute {
              method: to_method_type(route.method),
              path: route.path,
            })
            .collect(),
        )
      }
    } else {
      None
    };

    let job_handlers: Option<Vec<_>> = if args
      .subsystems
      .as_ref()
      .is_none_or(|c| c.contains(&trailbase_wasm_common::manifest::Subsystem::Jobs))
    {
      let handlers = T::job_handlers();
      if handlers.is_empty() {
        None
      } else {
        Some(
          handlers
            .into_iter()
            .map(|config| Job {
              name: config.name,
              spec: config.spec,
            })
            .collect(),
        )
      }
    } else {
      None
    };

    let sqlite_functions: Option<Vec<_>> = if args
      .subsystems
      .as_ref()
      .is_none_or(|c| c.contains(&trailbase_wasm_common::manifest::Subsystem::SqliteFunctions))
    {
      let handlers = Self::get_sqlite_scalar_functions();
      if handlers.is_empty() {
        None
      } else {
        Some(
          handlers
            .iter()
            .map(|f| {
              SqliteFunction::Scalar(SqliteScalarFunction {
                name: f.name.clone(),
                num_args: f.num_args,
                flags: f.flags.clone(),
              })
            })
            .collect(),
        )
      }
    } else {
      None
    };

    let admin_module: Option<AdminModule> = if args
      .subsystems
      .as_ref()
      .is_none_or(|c| c.contains(&trailbase_wasm_common::manifest::Subsystem::AdminModule))
    {
      T::admin_module()
    } else {
      None
    };

    let manifest = InitManifest {
      http_handlers,
      job_handlers,
      sqlite_functions,
      admin_module,
    };

    return serde_json::to_string(&manifest).map_err(|err| err.to_string());
  }
}

impl<T: Guest> crate::wit::exports::trailbase::component::sqlite_function_endpoint::Guest
  for TrailbaseHandler<T>
{
  fn dispatch_scalar_function(
    args: crate::wit::exports::trailbase::component::sqlite_function_endpoint::Arguments,
  ) -> Result<
    crate::wit::exports::trailbase::component::sqlite_function_endpoint::Value,
    crate::wit::exports::trailbase::component::sqlite_function_endpoint::Error,
  > {
    use crate::wit::exports::trailbase::component::sqlite_function_endpoint::Error;

    // QUESTION: This now initializes everything :/ - Does this need fixing?
    let f = Self::get_sqlite_scalar_functions()
      .iter()
      .find(|f| f.name == args.function_name)
      .ok_or_else(|| Error::Other("Missing function".to_string()))?;

    return (f.handler)(args.arguments);
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

fn to_method_type(m: Method) -> trailbase_wasm_common::manifest::HttpMethodType {
  use trailbase_wasm_common::manifest::HttpMethodType;

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
    _ => panic!("unknown http method type: {m}"),
  };
}
