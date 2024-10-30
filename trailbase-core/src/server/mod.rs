mod init;

use axum::extract::{DefaultBodyLimit, Request, State};
use axum::handler::HandlerWithoutStateExt;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{RequestExt, Router};
use rust_embed::RustEmbed;
use std::path::PathBuf;
use tokio::signal;
use tokio::task::JoinSet;
use tower_cookies::CookieManagerLayer;
use tower_http::{cors, limit::RequestBodyLimitLayer, services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{filter, prelude::*};

use crate::admin;
use crate::app_state::AppState;
use crate::assets::AssetService;
use crate::auth::util::is_admin;
use crate::auth::{self, AuthError, User};
use crate::constants::{AUTH_API_PATH, HEADER_CSRF_TOKEN, QUERY_API_PATH, RECORD_API_PATH};
use crate::data_dir::DataDir;
use crate::logging;
use crate::scheduler;

pub use init::{init_app_state, InitError};

/// A set of options to configure serving behaviors. Changing any of these options
/// requires a server restart, which makes them a natural fit for being exposed as command line
/// arguments.
#[derive(Debug, Clone, Default)]
pub struct ServerOptions {
  /// Optional path to static assets that will be served at the HTTP root.
  pub data_dir: DataDir,

  // Address the HTTP server binds to (Default: localhost:4000).
  pub address: String,

  // Optional address of the admin UI + API.
  pub admin_address: Option<String>,

  /// Optional path to static assets that will be served at the HTTP root.
  pub public_dir: Option<PathBuf>,

  /// Enabling dev mode allows free-for-all access to admin APIs. This can be useful to develop the
  /// UI behind a different server preventing auth cookie passing.
  ///
  /// NOTE: We might want to consider passing explicit auth headers when logging in specifically
  /// from the dev Admin UI.
  pub dev: bool,

  /// Disable the built-in public authentication (login, logout, ...) UI.
  pub disable_auth_ui: bool,

  /// Limit the set of allowed origins the HTTP server will answer to.
  pub cors_allowed_origins: Vec<String>,
}

pub struct Server {
  state: AppState,

  // Routers.
  main_router: (String, Router),
  admin_router: Option<(String, Router)>,
}

impl Server {
  /// Initializes the server. Will create a new data directory on first start.
  pub async fn init(opts: ServerOptions) -> Result<Self, InitError> {
    let (_, state) =
      init::init_app_state(opts.data_dir.clone(), opts.public_dir.clone(), opts.dev).await?;

    let main_router = Self::build_main_router(&state, &opts, None).await;
    let admin_router = Self::build_independent_admin_router(&state, &opts);

    Ok(Self {
      state,
      main_router,
      admin_router,
    })
  }

  /// Initializes the server in a more customizable manner. Will create a new data directory on
  /// first start.
  ///
  /// The `custom_routes` will be registered with the http server and `on_first_init` will be
  /// called only when a new data directory and therefore databases are created. This hook can
  /// be used to customize the setup in a simple manner, e.g. create tables, etc.
  /// Note, however, that for a multi-stage deployment (dev, test, staging, prod, ...) or prod
  /// setups migrations are a more robust approach to consistent and continuous management of
  /// schemas.
  pub async fn init_with_custom_routes_and_initializer<O>(
    opts: ServerOptions,
    custom_routes: Option<Router<AppState>>,
    on_first_init: impl FnOnce(AppState) -> O,
  ) -> Result<Self, InitError>
  where
    O: std::future::Future<Output = Result<(), Box<dyn std::error::Error + Sync + Send>>>,
  {
    let (new_data_dir, state) =
      init::init_app_state(opts.data_dir.clone(), opts.public_dir.clone(), opts.dev).await?;
    if new_data_dir {
      on_first_init(state.clone())
        .await
        .map_err(|err| InitError::CustomInit(err.to_string()))?;
    }

    let main_router = Self::build_main_router(&state, &opts, custom_routes).await;
    let admin_router = Self::build_independent_admin_router(&state, &opts);

    Ok(Self {
      state,
      main_router,
      admin_router,
    })
  }

  pub fn state(&self) -> &AppState {
    return &self.state;
  }

  pub fn router(&self) -> &Router<()> {
    return &self.main_router.1;
  }

  pub async fn serve(&self) -> Result<(), Box<dyn std::error::Error>> {
    // This declares **where** tracing is being logged to, e.g. stderr, file, sqlite.
    //
    // NOTE: it's ok to fail. Just means someone else already initialize the tracing sub-system.
    let _ = tracing_subscriber::registry()
      .with(
        logging::SqliteLogLayer::new(&self.state).with_filter(
          filter::Targets::new()
            .with_target("tower_http::trace::on_response", filter::LevelFilter::DEBUG)
            .with_target("tower_http::trace::on_request", filter::LevelFilter::DEBUG)
            .with_target("tower_http::trace::make_span", filter::LevelFilter::DEBUG)
            .with_default(filter::LevelFilter::INFO),
        ),
      )
      .try_init();

    let _raii_tasks = scheduler::start_periodic_tasks(&self.state);

    let mut set = JoinSet::new();

    {
      let (addr, router) = self.main_router.clone();
      set.spawn(async move { Self::start_listener(&addr, router).await });
    }

    if let Some((addr, router)) = self.admin_router.clone() {
      set.spawn(async move { Self::start_listener(&addr, router).await });
    }

    log::info!(
      "listening on http://{addr} 🚀 (Admin UI http://{admin_addr}/_/admin/)",
      addr = self.main_router.0,
      admin_addr = self
        .admin_router
        .as_ref()
        .map_or_else(|| &self.main_router.0, |(addr, _)| addr)
    );

    set.join_all().await;

    return Ok(());
  }

  async fn start_listener(addr: &str, router: Router<()>) -> std::io::Result<()> {
    let listener = match tokio::net::TcpListener::bind(addr).await {
      Ok(listener) => listener,
      Err(err) => {
        log::error!("Failed to listen on: {addr}: {err}");
        std::process::exit(1);
      }
    };

    if let Err(err) = axum::serve(listener, router.clone())
      .with_graceful_shutdown(shutdown_signal())
      .await
    {
      log::error!("Failed to start server: {err}");
      std::process::exit(1);
    }

    return Ok(());
  }

  fn build_admin_router(state: &AppState) -> Router<AppState> {
    return Router::new()
      .nest(
        "/api/_admin/",
        admin::router().layer(middleware::from_fn_with_state(
          state.clone(),
          assert_admin_api_access,
        )),
      )
      .nest_service(
        "/_/admin",
        AssetService::<AdminAssets>::with_parameters(
          // SPA-style fallback.
          Some(Box::new(|_| Some("index.html".to_string()))),
          Some("index.html".to_string()),
        ),
      );
  }

  fn build_independent_admin_router(
    state: &AppState,
    opts: &ServerOptions,
  ) -> Option<(String, Router<()>)> {
    let address = opts.admin_address.as_ref()?;
    if !has_indepenedent_admin_router(opts) {
      return None;
    }

    let router = Router::new()
      .nest(&format!("/{AUTH_API_PATH}"), auth::admin_auth_router())
      .nest("/", Self::build_admin_router(state));

    return Some((
      address.clone(),
      Self::wrap_with_default_layers(state, opts, router),
    ));
  }

  async fn build_main_router(
    state: &AppState,
    opts: &ServerOptions,
    custom_router: Option<Router<AppState>>,
  ) -> (String, Router<()>) {
    let mut router = Router::new()
      // Public, stable and versioned APIs.
      .nest(&format!("/{RECORD_API_PATH}"), crate::records::router())
      .nest(&format!("/{QUERY_API_PATH}"), crate::query::router())
      .nest(&format!("/{AUTH_API_PATH}"), auth::router())
      .route("/api/healthcheck", get(healthcheck_handler));

    if !has_indepenedent_admin_router(opts) {
      router = router.nest("/", Self::build_admin_router(state));
    }

    if !opts.disable_auth_ui {
      router = router.nest("/_/auth", crate::auth::auth_ui_router());
    }

    if let Some(custom_router) = custom_router {
      router = router.nest("/", custom_router);
    }

    if let Some(public_dir) = &opts.public_dir {
      if !tokio::fs::try_exists(public_dir).await.unwrap_or(false) {
        panic!("--public_dir={public_dir:?} path does not exist.")
      }

      async fn handle_404() -> (StatusCode, &'static str) {
        (StatusCode::NOT_FOUND, "Not found")
      }

      router = router
        .fallback_service(ServeDir::new(public_dir).not_found_service(handle_404.into_service()));
    }

    return (
      opts.address.clone(),
      Self::wrap_with_default_layers(state, opts, router),
    );
  }

  fn wrap_with_default_layers(
    state: &AppState,
    opts: &ServerOptions,
    router: Router<AppState>,
  ) -> Router<()> {
    return router
      .layer(CookieManagerLayer::new())
      .layer(build_cors(opts))
      .layer(
        // This declares: **what information** is logged at what level in to events and spans.
        TraceLayer::new_for_http()
          .make_span_with(logging::sqlite_logger_make_span)
          .on_request(logging::sqlite_logger_on_request)
          .on_response(logging::sqlite_logger_on_response),
      )
      // Default is only 2MB Increase to 10MB.
      .layer(DefaultBodyLimit::disable())
      .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024))
      .with_state(state.clone());
  }
}

fn has_indepenedent_admin_router(opts: &ServerOptions) -> bool {
  return match opts.admin_address {
    None => false,
    Some(ref address) if *address == opts.address => false,
    _ => true,
  };
}

async fn healthcheck_handler() -> Response {
  return (StatusCode::OK, "Ok").into_response();
}

/// Assert that the caller is an admin and provides a valid CSRF token. Unlike the access to the
/// HTML/js assets, this one errors.
///
/// NOTE: returning a redirect (like below) only makes sense for the html serving, not the APIs.
async fn assert_admin_api_access(
  State(state): State<AppState>,
  mut req: Request,
  next: Next,
) -> Result<Response, AuthError> {
  let user = req.extract_parts_with_state::<User, _>(&state).await?;

  if !is_admin(&state, &user).await {
    return Err(AuthError::Forbidden);
  }

  // CSRF protection.
  let Some(received_csrf_token) = req
    .headers()
    .get(HEADER_CSRF_TOKEN)
    .and_then(|header| header.to_str().ok())
  else {
    return Err(AuthError::BadRequest("admin APIs require csrf header"));
  };

  let expected_csrf = &user.csrf_token;
  if expected_csrf != received_csrf_token {
    return Err(AuthError::BadRequest("invalid CSRF token"));
  }

  return Ok(next.run(req).await);
}

fn build_cors(opts: &ServerOptions) -> cors::CorsLayer {
  if opts.dev {
    return cors::CorsLayer::very_permissive();
  }

  let origin_strs = &opts.cors_allowed_origins;
  let wildcard = origin_strs.iter().any(|s| s == "*");

  let origins = if wildcard {
    log::info!("CORS: allow any origin");
    // cors::AllowOrigin::any()
    cors::AllowOrigin::mirror_request()
  } else {
    cors::AllowOrigin::list(origin_strs.iter().filter_map(|o| {
      match HeaderValue::from_str(o.as_str()) {
        Ok(value) => Some(value),
        Err(err) => {
          log::error!("Invalid CORS origin {o}: {err}");
          None
        }
      }
    }))
  };

  // Cannot combine `Access-Control-Allow-Credentials: true` with `Access-Control-Allow-Methods: *`
  return cors::CorsLayer::new()
    .allow_methods(cors::Any)
    // .allow_credentials(wildcard)
    .allow_origin(origins);
}

async fn shutdown_signal() {
  let ctrl_c = async {
    signal::ctrl_c()
      .await
      .expect("failed to install Ctrl+C handler");
  };

  #[cfg(unix)]
  let terminate = async {
    signal::unix::signal(signal::unix::SignalKind::terminate())
      .expect("failed to install signal handler")
      .recv()
      .await;
  };

  #[cfg(not(unix))]
  let terminate = std::future::pending::<()>();

  tokio::select! {
      _ = ctrl_c => {
      println!("Received Ctrl+C. Shutting down gracefully.");
    },
      _ = terminate => {
      println!("Received termination. Shutting down gracefully.");
    },
  }
}

#[derive(RustEmbed, Clone)]
#[folder = "../ui/admin/dist/"]
struct AdminAssets;
