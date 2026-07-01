use axum::{
  extract::{FromRef, State},
  response::{Html, IntoResponse, Response},
  routing::{Router, get},
};
use trailbase::api::{InitArgs, init_app_state};
use trailbase::{AppState, DataDir, Server, ServerOptions, User};

#[derive(Clone)]
struct CustomState {
  state: AppState,
  greeting: Option<String>,
}

impl FromRef<CustomState> for AppState {
  fn from_ref(s: &CustomState) -> Self {
    s.state.clone()
  }
}

async fn hello_world_handler(State(state): State<CustomState>, user: Option<User>) -> Response {
  let greeting = state.greeting.as_deref().unwrap_or("Hello");
  let subject = match user {
    Some(ref user) => user
      .email
      .as_deref()
      .or(user.username.as_deref())
      .unwrap_or("World"),
    None => "World",
  };

  Html(format!("<p>{greeting}, {subject}!</p>")).into_response()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  env_logger::init_from_env(
    env_logger::Env::new()
      .default_filter_or("info,trailbase_refinery=warn,tracing::span=warn,swc_ecma_codegen=off"),
  );

  // Install the process-wide rustls crypto provider. Since rustls 0.23.39 there is no more
  // implicit default. W/o this any TLS traffic incoming and outgoing (e.g. via WASM components)
  // would panic.
  tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
    .install_default()
    .expect("Failed to install rustls crypto");

  let (new_db, state) = init_app_state(InitArgs {
    data_dir: DataDir::default(),
    public_dir: None,
    dev: false,
    ..Default::default()
  })
  .await?;

  if new_db {
    println!("Fresh data dir initialized: {:?}", state.data_dir());
  }

  let server = Server::init(
    state.clone(),
    ServerOptions {
      address: "localhost:4004".to_string(),
      admin_address: None,
      public_dir: None,
      log_responses: true,
      cors_allowed_origins: vec![],
      custom_router: Some(
        Router::new()
          .route("/", get(hello_world_handler))
          .with_state(CustomState {
            state: state.clone(),
            greeting: Some("Hi".to_string()),
          }),
      ),
      ..Default::default()
    },
  )
  .await?;

  server.serve().await?;

  Ok(())
}
