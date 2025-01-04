use axum::{
  extract::State,
  response::{Html, IntoResponse, Response},
  routing::{get, Router},
};
use tracing_subscriber::{filter, prelude::*};
use trailbase::{AppState, DataDir, Server, ServerOptions, User};

type BoxError = Box<dyn std::error::Error>;

pub async fn handler(State(_state): State<AppState>, user: Option<User>) -> Response {
  Html(format!(
    "<p>Hello, {}!</p>",
    user.map_or("World".to_string(), |user| user.email)
  ))
  .into_response()
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
  env_logger::init_from_env(
    env_logger::Env::new()
      .default_filter_or("info,refinery_core=warn,tracing::span=warn,swc_ecma_codegen=off"),
  );

  let custom_routes: Router<AppState> = Router::new().route("/", get(handler));

  let app = Server::init_with_custom_routes_and_initializer(
    ServerOptions {
      data_dir: DataDir::default(),
      address: "localhost:4004".to_string(),
      admin_address: None,
      public_dir: None,
      dev: false,
      disable_auth_ui: false,
      cors_allowed_origins: vec![],
      js_runtime_threads: None,
    },
    Some(custom_routes),
    |state: AppState| async move {
      println!("Data dir: {:?}", state.data_dir());
      Ok(())
    },
  )
  .await?;

  // This declares **where** tracing is being logged to, e.g. stderr, file, sqlite.
  let _layer = tracing_subscriber::registry()
    .with(
      trailbase::logging::SqliteLogLayer::new(app.state()).with_filter(filter::LevelFilter::INFO),
    )
    .with(
      tracing_subscriber::fmt::layer().compact().with_filter(
        // Limit messages to INFO and above except for request handling logs.
        filter::Targets::new()
          .with_target("tower_http::trace::on_response", filter::LevelFilter::DEBUG)
          .with_target("tower_http::trace::on_request", filter::LevelFilter::DEBUG)
          .with_target("tower_http::trace::make_span", filter::LevelFilter::DEBUG)
          .with_default(filter::LevelFilter::INFO),
      ),
    )
    .try_init();

  app.serve().await?;

  Ok(())
}
