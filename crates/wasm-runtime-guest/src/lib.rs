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

pub use crate::wit::exports::trailbase::runtime::init_endpoint::{Guest, InitResult};

use trailbase_wasm_common::{SqliteRequest, SqliteResponse};
use wstd::http::body::{BoundedBody, IntoBody};
use wstd::http::{Client, Request};

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
  let response: SqliteResponse = serde_json::from_slice(&bytes).expect("bar");

  if let Some(err) = response.error {
    return Err(Error::Sqlite(err));
  }

  return Ok(response.rows);
}
