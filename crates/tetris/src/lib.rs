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
      icon: Some(ICON.to_string()),
      admin_ui_path: Some("/tetris/".to_string()),
      description: Some("Example WASM component for playing Tetris.".to_string()),
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

const ICON: &str = r##"<svg width="800px" height="800px" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
  <path d="M9.18 12.75H3.53C3.33189 12.7474 3.14263 12.6675 3.00253 12.5275C2.86244 12.3874 2.78259 12.1981 2.78 12V6.51999C2.78 6.32108 2.85902 6.13031 2.99967 5.98966C3.14032 5.84901 3.33109 5.76999 3.53 5.76999H9.18C9.37891 5.76999 9.56968 5.84901 9.71033 5.98966C9.85098 6.13031 9.93 6.32108 9.93 6.51999V12C9.92741 12.1981 9.84756 12.3874 9.70747 12.5275C9.56737 12.6675 9.37811 12.7474 9.18 12.75ZM4.28 11.25H8.43V7.24999H4.28V11.25Z" fill="#000000"/>
  <path d="M14.82 12.75H9.18C8.98109 12.75 8.79032 12.671 8.64967 12.5303C8.50902 12.3897 8.43 12.1989 8.43 12V6.52C8.42865 6.42113 8.44714 6.323 8.48435 6.2314C8.52156 6.1398 8.57676 6.05658 8.64667 5.98667C8.71659 5.91676 8.7998 5.86156 8.8914 5.82435C8.983 5.78713 9.08114 5.76865 9.18 5.77H14.82C14.9189 5.76865 15.017 5.78713 15.1086 5.82435C15.2002 5.86156 15.2834 5.91676 15.3533 5.98667C15.4232 6.05658 15.4784 6.1398 15.5156 6.2314C15.5529 6.323 15.5713 6.42113 15.57 6.52V12C15.57 12.1989 15.491 12.3897 15.3503 12.5303C15.2097 12.671 15.0189 12.75 14.82 12.75ZM9.93 11.25H14.07V7.25H9.93V11.25Z" fill="#000000"/>
  <path d="M20.47 12.75H14.82C14.6219 12.7474 14.4326 12.6675 14.2925 12.5275C14.1524 12.3874 14.0726 12.1981 14.07 12V6.51999C14.07 6.32108 14.149 6.13031 14.2897 5.98966C14.4303 5.84901 14.6211 5.76999 14.82 5.76999H20.47C20.6689 5.76999 20.8597 5.84901 21.0003 5.98966C21.141 6.13031 21.22 6.32108 21.22 6.51999V12C21.2174 12.1981 21.1376 12.3874 20.9975 12.5275C20.8574 12.6675 20.6681 12.7474 20.47 12.75ZM15.57 11.25H19.72V7.24999H15.57V11.25Z" fill="#000000"/>
  <path d="M14.82 18.23H9.18C9.08114 18.2313 8.983 18.2129 8.8914 18.1756C8.7998 18.1384 8.71659 18.0832 8.64667 18.0133C8.57676 17.9434 8.52156 17.8602 8.48435 17.7686C8.44714 17.677 8.42865 17.5789 8.43 17.48V12C8.43 11.8011 8.50902 11.6103 8.64967 11.4697C8.79032 11.329 8.98109 11.25 9.18 11.25H14.82C15.0189 11.25 15.2097 11.329 15.3503 11.4697C15.491 11.6103 15.57 11.8011 15.57 12V17.48C15.5713 17.5789 15.5529 17.677 15.5156 17.7686C15.4784 17.8602 15.4232 17.9434 15.3533 18.0133C15.2834 18.0832 15.2002 18.1384 15.1086 18.1756C15.017 18.2129 14.9189 18.2313 14.82 18.23ZM9.93 16.73H14.07V12.73H9.93V16.73Z" fill="#000000"/>
</svg>"##;
