#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use trailbase_wasm_guest::{HttpError, HttpRoute, Method, export};
use wstd::http::StatusCode;

// Implement the function exported in this world (see above).
struct Endpoints;

impl trailbase_wasm_guest::Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![HttpRoute::new(
      Method::GET,
      "/error",
      async |_req| -> Result<(), HttpError> {
        return Err(HttpError {
          status: StatusCode::IM_A_TEAPOT,
          message: Some("I'm a teapot".to_string()),
        });
      },
    )];
  }
}

export!(Endpoints);
