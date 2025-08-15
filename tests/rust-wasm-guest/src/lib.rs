use wstd::http::body::{BodyForthcoming, IncomingBody};
use wstd::http::server::{Finished, Responder};
use wstd::http::{Request, Response, StatusCode};
use wstd::io::{AsyncWrite, empty};

// Implement the function exported in this world (see above).
struct InitEndpoint;

impl trailbase_wasm_guest::Guest for InitEndpoint {
  fn init() -> trailbase_wasm_guest::InitResult {
    println!("init() called");
    return trailbase_wasm_guest::InitResult {
      http_handlers: vec![],
      job_handlers: vec![],
    };
  }
}

trailbase_wasm_guest::wit::export!(InitEndpoint);

// TODO: Ship our own macro when making rust available as a supported guest language.
#[wstd::http_server]
async fn main(request: Request<IncomingBody>, responder: Responder) -> Finished {
  return match request.uri().path() {
    "/" => {
      let msg = std::future::ready("Hello! - from root HTTP handler").await;
      println!("{msg}");

      let rows = trailbase_wasm_guest::query("SELECT COUNT(*) FROM TEST", vec![])
        .await
        .unwrap();
      if rows[0][0] != serde_json::json!(1) {
        panic!("Expected one");
      }

      let mut body = responder.start_response(Response::new(BodyForthcoming));
      let result = body
        .write_all(format!("response: {rows:?}").as_bytes())
        .await;

      Finished::finish(body, result, None)
    }
    _ => {
      let response = Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(empty())
        .unwrap();
      responder.respond(response).await
    }
  };
}
