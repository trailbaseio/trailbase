#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use trailbase_wasm::db::{Value, query};
use trailbase_wasm::http::{HttpError, HttpRoute, Json, Request, StatusCode, routing};
use trailbase_wasm::{Guest, export};

type SearchResponse = (String, f64, f64, f64, f64);

async fn search_handler(req: Request) -> Result<Json<Vec<SearchResponse>>, HttpError> {
  let (mut aroma, mut flavor, mut acidity, mut sweetness) = (8, 8, 8, 8);

  for (param, value) in req.url().query_pairs() {
    match param.as_ref() {
      "aroma" => aroma = value.parse().unwrap_or(aroma),
      "flavor" => flavor = value.parse().unwrap_or(flavor),
      "acidity" => acidity = value.parse().unwrap_or(acidity),
      "sweetness" => sweetness = value.parse().unwrap_or(sweetness),
      _ => {}
    }
  }

  // Query the closest match using vector-search.
  let results: Vec<SearchResponse> = query(
    r#"
      SELECT Owner, Aroma, Flavor, Acidity, Sweetness
        FROM coffee
        ORDER BY vec_distance_L2(
          embedding, FORMAT("[%f, %f, %f, %f]", $1, $2, $3, $4))
        LIMIT 100
    "#,
    [
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
      panic!("invariant");
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

fn as_real(v: &Value) -> Result<f64, String> {
  return match v {
    Value::Real(f) => Ok(*f),
    _ => Err(format!("Not a real: {v:?}")),
  };
}

// Lastly, implement and export a TrailBase component.
struct GuestImpl;

impl Guest for GuestImpl {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![routing::get("/search", search_handler)];
  }
}

export!(GuestImpl);
