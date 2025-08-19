#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]
use trailbase_wasm_guest::{HttpIncomingHandler, Method, to_handler};

// Implement the function exported in this world (see above).
struct InitEndpoint;

impl trailbase_wasm_guest::Init for InitEndpoint {
  fn http_handlers() -> Vec<(
    wstd::http::Method,
    &'static str,
    trailbase_wasm_guest::Handler,
  )> {
    let thread_id = trailbase_wasm_guest::thread_id();
    println!("http_handlers() called (thread: {thread_id})");

    return vec![
      (
        Method::GET,
        "/wasm",
        to_handler(async |_req| Ok(b"Welcome from WASM\n".to_vec())),
      ),
      (
        Method::GET,
        "/fibonacci",
        to_handler(async |_req| Ok(format!("{}\n", fibonacci(40)).as_bytes().to_vec())),
      ),
    ];
  }
}

::trailbase_wasm_guest::export!(InitEndpoint);

#[inline]
fn fibonacci(n: usize) -> usize {
  return match n {
    0 => 0,
    1 => 1,
    n => fibonacci(n - 1) + fibonacci(n - 2),
  };
}
