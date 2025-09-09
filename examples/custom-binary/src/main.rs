use axum::{
  extract::{FromRef, State},
  response::{Html, IntoResponse, Response},
  routing::{Router, get},
};
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
  let subject = user.map_or("World".to_string(), |user| user.email);

  Html(format!("<p>{greeting}, {subject}!</p>")).into_response()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  env_logger::init_from_env(
    env_logger::Env::new()
      .default_filter_or("info,trailbase_refinery=warn,tracing::span=warn,swc_ecma_codegen=off"),
  );

  let Server {
    state,
    main_router,
    admin_router,
    tls,
  } = Server::init_with_custom_initializer(
    ServerOptions {
      data_dir: DataDir::default(),
      address: "localhost:4004".to_string(),
      admin_address: None,
      public_dir: None,
      log_responses: true,
      dev: false,
      disable_auth_ui: false,
      cors_allowed_origins: vec![],
      ..Default::default()
    },
    |state: AppState| async move {
      println!("Fresh data dir initialized: {:?}", state.data_dir());
      Ok(())
    },
  )
  .await?;

  let router = {
    let custom_router = Router::new()
      .route("/", get(hello_world_handler))
      .with_state(CustomState {
        state,
        greeting: Some("Hi".to_string()),
      })
      .merge(main_router.1);

    (main_router.0, custom_router)
  };

  trailbase::api::serve(router, admin_router, tls).await?;

  Ok(())
}
