use wstd::http::body::{BodyForthcoming, IncomingBody};
use wstd::http::server::{Finished, Responder};
use wstd::http::{Request, Response, StatusCode};
use wstd::io::{AsyncWrite, empty};

wit_bindgen::generate!({
    world: "trailbase:runtime/trailbase",
    path: [
        // Order-sensitive: will import *.wit from the folder.
        "../../crates/wasm-runtime/wit/deps-0.2.6/random",
        "../../crates/wasm-runtime/wit/deps-0.2.6/io",
        "../../crates/wasm-runtime/wit/deps-0.2.6/clocks",
        "../../crates/wasm-runtime/wit/deps-0.2.6/filesystem",
        "../../crates/wasm-runtime/wit/deps-0.2.6/sockets",
        "../../crates/wasm-runtime/wit/deps-0.2.6/cli",
        "../../crates/wasm-runtime/wit/deps-0.2.6/http",
        "../../crates/wasm-runtime/wit/trailbase.wit",
    ],
    generate_all,
});

// Implement the function exported in this world (see above).
struct InitEndpoint;

impl crate::exports::trailbase::runtime::init_endpoint::Guest for InitEndpoint {
  fn init() {
    println!("init() called");
  }
}

export!(InitEndpoint);

async fn http_not_found(_request: Request<IncomingBody>, responder: Responder) -> Finished {
  let response = Response::builder()
    .status(StatusCode::NOT_FOUND)
    .body(empty())
    .unwrap();
  responder.respond(response).await
}

#[wstd::http_server]
async fn main(request: Request<IncomingBody>, responder: Responder) -> Finished {
  match request.uri().path_and_query().unwrap().as_str() {
    "/" => {
      let msg = std::future::ready("Hello! - from root HTTP handler").await;
      println!("{msg}");

      let mut body = responder.start_response(Response::new(BodyForthcoming));
      let result = body.write_all(b"response").await;
      Finished::finish(body, result, None)
    }
    _ => http_not_found(request, responder).await,
  }
}
