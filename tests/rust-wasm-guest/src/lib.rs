use trailbase_wasm_guest::MethodType;
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
      http_handlers: vec![(MethodType::Get, "/wasm".to_string())],
      job_handlers: vec![],
    };
  }
}

trailbase_wasm_guest::wit::export!(InitEndpoint);

// TODO: Ship our own macro when making rust available as a supported guest language.
#[wstd::http_server]
async fn main(request: Request<IncomingBody>, responder: Responder) -> Finished {
  println!("Hello from WASM guest: {}", request.uri().path());
  return match request.uri().path() {
    // TODO: Build an abstraction to sync init handlers with http handlers here both for http and
    // jobs..
    "/wasm" => write_all(responder, format!("Welcome from WASM").as_bytes()).await,
    "/query" => {
      let query = std::future::ready("SELECT COUNT(*) FROM TEST").await;
      let rows = trailbase_wasm_guest::query(&query, vec![]).await.unwrap();
      if rows[0][0] != serde_json::json!(1) {
        panic!("Expected one");
      }

      write_all(responder, format!("response: {rows:?}").as_bytes()).await
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

async fn write_all(responder: Responder, buf: &[u8]) -> Finished {
  let mut body = responder.start_response(Response::new(BodyForthcoming));
  let result = body.write_all(buf).await;
  Finished::finish(body, result, None)
}
