#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use rust_embed::RustEmbed;
use trailbase_wasm::http::{
  HttpError, HttpRoute, IntoBody, Request, Response, StatusCode, header, routing,
};
use trailbase_wasm::{Guest, Metadata, export};

// Implement the function exported in this world (see above).
struct Endpoints;

impl Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![
      routing::get("/tetris/", async |_req: Request| {
        return static_assets_handler("").await;
      }),
      routing::get("/tetris/{*wildcard}", async |req: Request| {
        return static_assets_handler(
          req
            .path_param("wildcard")
            .ok_or_else(|| internal("missing param"))?,
        )
        .await;
      }),
    ];
  }

  fn metadata() -> Option<Metadata> {
    return Some(Metadata {
      display_name: Some("Tetris".to_string()),
      admin_ui_path: Some("/tetris/".to_string()),
      ..Default::default()
    });
  }
}

export!(Endpoints);

async fn static_assets_handler(path: &str) -> Result<Response, HttpError> {
  // We want as little magic as possible. The only /_/auth/subpath that isn't SSR, is
  // profile, so we when hitting /profile or /profile, we want actually want to serve
  // the static profile/index.html.
  let file = match path {
    "" => Assets::get("index.html"),
    p => Assets::get(p),
  }
  .ok_or_else(|| HttpError::message(StatusCode::NOT_FOUND, "Not found"))?;

  let response_builder = Response::builder()
    .header(
      header::CONTENT_TYPE,
      match path {
        p if p.ends_with(".js") => "text/javascript",
        p if p.ends_with(".css") => "text/css",
        p if p.ends_with(".html") => "text/html",
        _ => file.metadata.mimetype(),
      },
    )
    .header(header::CACHE_CONTROL, "public")
    .header(header::CACHE_CONTROL, "max-age=604800")
    .header(header::CACHE_CONTROL, "immutable");

  return response_builder
    .body(file.data.into_body())
    .map_err(internal);
}

#[inline]
fn internal(err: impl std::string::ToString) -> HttpError {
  return HttpError::message(StatusCode::INTERNAL_SERVER_ERROR, err);
}

#[derive(RustEmbed, Clone)]
#[folder = "assets/"]
pub struct Assets;
