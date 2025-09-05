#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use trailbase_wasm::db::{self, Value, query};
use trailbase_wasm::http::{HttpError, HttpRoute, Method, Request, StatusCode};
use trailbase_wasm::{Guest, export};

async fn search_handler(req: Request) -> Result<String, HttpError> {
  let mut aroma: i64 = 8;
  let mut flavor: i64 = 8;
  let mut acidity: i64 = 8;
  let mut sweetness: i64 = 8;

  for (param, value) in req.url().query_pairs() {
    match param.as_ref() {
      "aroma" => aroma = value.parse().unwrap_or(aroma),
      "flavor" => flavor = value.parse().unwrap_or(flavor),
      "acidity" => acidity = value.parse().unwrap_or(acidity),
      "sweetness" => sweetness = value.parse().unwrap_or(sweetness),
      _ => {}
    }
  }

  // Query with vector-search for the closest match.
  let rows: Vec<Vec<Value>> = query(
    r#"
      SELECT Owner, Aroma, Flavor, Acidity, Sweetness
        FROM coffee
        ORDER BY vec_distance_L2(
          embedding, FORMAT("[%f, %f, %f, %f]", $1, $2, $3, $4))
        LIMIT 100
    "#
    .to_string(),
    vec![
      Value::Integer(aroma),
      Value::Integer(flavor),
      Value::Integer(acidity),
      Value::Integer(sweetness),
    ],
  )
  .await
  .map_err(internal)?;

  return Ok(
    serde_json::to_string(
      &rows
        .into_iter()
        .map(|row| {
          row
            .into_iter()
            .map(|v| db::to_json_value(v))
            .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>(),
    )
    .map_err(internal)?,
  );
}

// Implement the function exported in this world (see above).
struct Endpoints;

impl Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![HttpRoute::new(Method::GET, "/search", search_handler)];
  }
}

export!(Endpoints);

fn internal(err: impl std::string::ToString) -> HttpError {
  return HttpError::message(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
}
