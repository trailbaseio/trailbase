#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use trailbase_wasm::db::{Value, query};
use trailbase_wasm::http::{HttpError, HttpRoute, Json, Request, StatusCode, routing};
use trailbase_wasm::{Guest, export};

type SearchResponse = (String, f64, f64, f64, f64);

fn as_real(v: &Value) -> Result<f64, String> {
  if let Value::Real(f) = v {
    return Ok(*f);
  }
  return Err(format!("Not a real: {v:?}"));
}

async fn search_handler(req: Request) -> Result<Json<Vec<SearchResponse>>, HttpError> {
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
  let results: Vec<SearchResponse> = query(
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
  .map_err(|err| HttpError::message(StatusCode::INTERNAL_SERVER_ERROR, err))?
  .into_iter()
  .map(|row| {
    // Convert to json response.
    let Value::Text(owner) = row[0].clone() else {
      panic!("unreachable");
    };

    return (
      owner,
      as_real(&row[1]).expect("invariant"),
      as_real(&row[2]).expect("invariant"),
      as_real(&row[3]).expect("invariant"),
      as_real(&row[4]).expect("invariant"),
    );
  })
  .collect();

  return Ok(Json(results));
}

// Implement the function exported in this world (see above).
struct Endpoints;

impl Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![routing::get("/search", search_handler)];
  }
}

export!(Endpoints);
