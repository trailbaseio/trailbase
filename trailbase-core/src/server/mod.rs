mod init;
mod serve;

use axum::extract::{DefaultBodyLimit, Request, State};
use axum::handler::HandlerWithoutStateExt;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{RequestExt, Router};
use log::*;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tokio::task::JoinSet;
use tokio_rustls::{
  TlsAcceptor,
  rustls::ServerConfig,
  rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use tower_cookies::CookieManagerLayer;
use tower_http::{cors, limit::RequestBodyLimitLayer, services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{filter, prelude::*};
use trailbase_assets::AssetService;

use crate::admin;
use crate::app_state::AppState;
use crate::auth::util::is_admin;
use crate::auth::{self, AuthError, User};
use crate::constants::{ADMIN_API_PATH, HEADER_CSRF_TOKEN};
use crate::data_dir::DataDir;
use crate::logging;
use crate::records;

pub use init::{InitArgs, InitError, init_app_state};

/// A set of options to configure serving behaviors. Changing any of these options
/// requires a server restart, which makes them a natural fit for being exposed as command line
/// arguments.
#[derive(Debug, Default)]
pub struct ServerOptions {
  /// Optional path to static assets that will be served at the HTTP root.
  pub data_dir: DataDir,

  // Authority (<host>:<port>) the HTTP server binds to, e.g. "localhost:4000".
  pub address: String,

  // Optional address of the admin UI + API.
  pub admin_address: Option<String>,

  /// Optional path to static assets that will be served at the HTTP root.
  pub public_dir: Option<PathBuf>,

  /// Optional path to MaxmindDB geoip database. Can be used to map logged IPs to a geo location.
  pub geoip_db_path: Option<PathBuf>,

  /// We trace the request->response flow to generate a server log. Setting this to true will also
  /// log an event to stdout.
  pub log_responses: bool,

  /// In dev mode CORS and cookies will be more permissive to allow development with externally
  /// hosted UIs, e.g. using a dev serer.
  pub dev: bool,

  // Enabling demo mode, e.g. to redact PII from Admin UI.
  pub demo: bool,

  /// Disable the built-in public authentication (login, logout, ...) UI.
  pub disable_auth_ui: bool,

  /// Limit the set of allowed origins the HTTP server will answer to.
  pub cors_allowed_origins: Vec<String>,

  /// Number of V8 worker threads. If set to None, default of num available cores will be used.
  pub js_runtime_threads: Option<usize>,

  /// TLS certificate path.
  pub tls_cert: Option<CertificateDer<'static>>,
  /// TLS key path.
  pub tls_key: Option<PrivateKeyDer<'static>>,
}

pub struct Server {
  pub state: AppState,

  // Routers.
  pub main_router: (String, Router),
  pub admin_router: Option<(String, Router)>,

  // TLS/SSL
  pub tls: Option<(CertificateDer<'static>, PrivateKeyDer<'static>)>,
}

impl Server {
  /// Initializes the server. Will create a new data directory on first start.
  pub async fn init(opts: ServerOptions) -> Result<Self, InitError> {
    return Self::init_with_custom_initializer(opts, |_| async { Ok(()) }).await;
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
  pub async fn init_with_custom_initializer(
    opts: ServerOptions,
    on_first_init: impl AsyncFnOnce(AppState) -> Result<(), Box<dyn std::error::Error + Sync + Send>>,
  ) -> Result<Self, InitError> {
    let version_info = trailbase_assets::get_version_info!();
    info!(
      "Initializing server version: {hash} {date}",
      hash = version_info.commit_hash.unwrap_or_default(),
      date = version_info.commit_date.unwrap_or_default(),
    );

    let (new_data_dir, state) = init::init_app_state(InitArgs {
      data_dir: opts.data_dir.clone(),
      public_dir: opts.public_dir.clone(),
      geoip_db_path: opts.geoip_db_path.clone(),
      address: opts.address.clone(),
      dev: opts.dev,
      demo: opts.demo,
      js_runtime_threads: opts.js_runtime_threads,
    })
    .await?;

    // Initialize tracing subscribers/layers.
    //
    // A few notes in case initialization below panics. The `log` and `tracing` crates/systems are
    // mostly independent. Both like to be initialized only once given their global nature. There
    // is a `.try_init()`, which has not effect when already initialized.
    //
    // Here we specifically only initialize `tracing`, since we critically rely on the
    // `SqliteLogLayer`. We leave `log` initialization to the program level.
    //
    // The current setup prevents users from initializing tracing themselves. This is only relevant
    // for the frameworks-use-case. If we wanted to allow it, we could check that if already
    // initialized, the "logging::SqliteLogLayer" is present.
    //
    // If the `tracing_subscriber` crate is built with the default feature `tracing-log`,
    // initializing `tracing` will also initialize the `log` crate. So this approach will only
    // work if built w/o `tracing-log`. Otherwise, initializing `log` before will lead to a panic
    // here. We do *not* want to use a `.try_init()` here, otherwise may silently miss
    // `SqliteLogLayer`.
    //
    // Response log events are emitted at the INFO level, see `logging.rs`
    let filter_layer = filter::Targets::new()
      .with_default(filter::LevelFilter::OFF)
      .with_target(crate::logging::EVENT_TARGET, crate::logging::LEVEL);

    tracing_subscriber::Registry::default()
      .with(filter_layer)
      .with(logging::SqliteLogLayer::new(
        &state,
        /* log-to-stdout= */ opts.log_responses,
      ))
      .init();

    if new_data_dir {
      on_first_init(state.clone())
        .await
        .map_err(|err| InitError::CustomInit(err.to_string()))?;
    }

    #[cfg(feature = "v8")]
    let js_routes: Option<Router<AppState>> =
      crate::js::runtime::load_routes_and_jobs_from_js_modules(&state)
        .await
        .map_err(|err| InitError::ScriptError(err.to_string()))?;

    #[cfg(not(feature = "v8"))]
    let js_routes: Option<Router<AppState>> = None;

    Ok(Self {
      state: state.clone(),
      main_router: Self::build_main_router(&state, &opts, js_routes).await,
      admin_router: Self::build_independent_admin_router(&state, &opts),
      tls: Self::load_tls(&opts),
    })
  }

  pub async fn serve(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Install a HUP/hangup signal handler to reload config.
    #[cfg(unix)]
    {
      let state = self.state.clone();
      tokio::spawn(async move {
        // An infinite stream of hangup signals.
        let mut stream = signal::unix::signal(signal::unix::SignalKind::hangup()).expect("startup");

        loop {
          stream.recv().await;
          log::info!("Received HUP signal. Reloading config.");

          match crate::config::load_or_init_config_textproto(
            state.data_dir(),
            state.schema_metadata(),
          )
          .await
          {
            Ok(config) => {
              if let Err(err) = state.validate_and_update_config(config, None).await {
                log::error!("Failed to reload config: {err}");
              }
            }
            Err(err) => {
              log::error!("Failed to reload config: {err}");
            }
          }
        }
      });
    }

    // Finally start serving.
    return serve(self.main_router, self.admin_router, self.tls).await;
  }

  pub fn load_tls(
    opts: &ServerOptions,
  ) -> Option<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    let data_dir = &opts.data_dir;
    let tls_cert = opts.tls_cert.clone().map_or_else(
      || {
        std::fs::read(data_dir.secrets_path().join("certs").join("cert.pem"))
          .ok()
          .and_then(|cert| CertificateDer::from_pem_slice(&cert).ok())
      },
      Some,
    );
    let tls_key = opts.tls_key.as_ref().map_or_else(
      || {
        std::fs::read(data_dir.secrets_path().join("certs").join("key.pem"))
          .ok()
          .and_then(|key| PrivateKeyDer::from_pem_slice(&key).ok())
      },
      |key| Some(key.clone_key()),
    );

    return match (tls_cert, tls_key) {
      (Some(cert), Some(key)) => Some((cert, key)),
      (Some(_cert), None) => {
        warn!("TLS cert provided but key missing");
        None
      }
      (None, Some(_key)) => {
        warn!("TLS key provided but cert missing");
        None
      }
      (None, None) => None,
    };
  }

  fn build_admin_router(state: &AppState) -> Router<AppState> {
    return Router::new()
      .nest(
        &format!("/{ADMIN_API_PATH}/"),
        admin::router().layer(middleware::from_fn_with_state(
          state.clone(),
          assert_admin_api_access,
        )),
      )
      .nest_service(
        "/_/admin",
        AssetService::<trailbase_assets::AdminAssets>::with_parameters(
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
      .merge(auth::admin_auth_router())
      .merge(Self::build_admin_router(state));

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
      .merge(records::router())
      .merge(auth::router())
      .route("/api/healthcheck", get(healthcheck_handler));

    if !has_indepenedent_admin_router(opts) {
      router = router.merge(Self::build_admin_router(state));
    }

    if !opts.disable_auth_ui {
      router = router.merge(auth::auth_ui_router());
    }

    if let Some(custom_router) = custom_router {
      router = router.merge(custom_router);
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
    info!("CORS: allow any origin");
    // cors::AllowOrigin::any()
    cors::AllowOrigin::mirror_request()
  } else {
    cors::AllowOrigin::list(origin_strs.iter().filter_map(|o| {
      match HeaderValue::from_str(o.as_str()) {
        Ok(value) => Some(value),
        Err(err) => {
          error!("Invalid CORS origin {o}: {err}");
          None
        }
      }
    }))
  };

  // Cannot combine `Access-Control-Allow-Credentials: true` with `Access-Control-Allow-Methods: *`
  return cors::CorsLayer::new()
    .allow_methods(cors::Any)
    .allow_headers(cors::Any)
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

  async fn timer() {
    use tokio::time::*;

    const SECONDS: usize = 10;

    for remaining in (0..SECONDS).rev() {
      tokio::select! {
        _ = sleep(Duration::from_secs(1)) => {}
        _ = signal::ctrl_c() => {
            println!("Got Ctrl+C. Shutting down");
            std::process::exit(1);
        }
      };

      if remaining > 0 {
        println!("Waiting {SECONDS}s for graceful shutdown: {remaining}s remaining.");
      } else {
        println!("Graceful shutdown failed. Shutting down");
        std::process::exit(0);
      }
    }
  }

  tokio::select! {
      _ = ctrl_c => {
      println!("Received Ctrl+C. Shutting down gracefully.");
      tokio::spawn(timer());
    },
      _ = terminate => {
      println!("Received termination. Shutting down gracefully.");
      tokio::spawn(timer());
    },
  }
}

pub async fn serve(
  main_router: (String, Router),
  admin_router: Option<(String, Router)>,
  tls: Option<(CertificateDer<'static>, PrivateKeyDer<'static>)>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  let has_tls = tls.is_some();
  let addr = main_router.0.clone();
  let admin_addr = admin_router
    .as_ref()
    .map_or_else(|| addr.clone(), |(addr, _)| addr.clone());

  let set = {
    let mut set = JoinSet::new();

    if let Some((addr, router)) = admin_router {
      let tls_clone = tls
        .as_ref()
        .map(|(cert, key)| (cert.clone(), key.clone_key()));
      set.spawn(async move { start_listen(&addr, router, tls_clone).await });
    }

    {
      let (addr, router) = main_router;
      set.spawn(async move { start_listen(&addr, router, tls).await });
    }

    set
  };

  info!(
    "Listening on {protocol}://{addr} 🚀 (Admin UI http://{admin_addr}/_/admin/)",
    protocol = if has_tls { "https" } else { "http" },
  );

  set.join_all().await;

  return Ok(());
}

async fn start_listen(
  addr: &str,
  router: Router<()>,
  tls: Option<(CertificateDer<'static>, PrivateKeyDer<'static>)>,
) {
  match tls {
    Some((cert, key)) => {
      let tcp_listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(err) => {
          error!("Failed to listen on: {addr}: {err}");
          std::process::exit(1);
        }
      };

      let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .expect("Failed to build server config");

      let listener = serve::TlsListener {
        listener: tcp_listener,
        acceptor: TlsAcceptor::from(Arc::new(server_config)),
      };

      if let Err(err) = serve::serve(listener, router.clone())
        .with_graceful_shutdown(shutdown_signal())
        .await
      {
        error!("Failed to start server: {err}");
        std::process::exit(1);
      }
    }
    _ => {
      let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(err) => {
          error!("Failed to listen on: {addr}: {err}");
          std::process::exit(1);
        }
      };

      if let Err(err) = serve::serve(listener, router.clone())
        .with_graceful_shutdown(shutdown_signal())
        .await
      {
        error!("Failed to start server: {err}");
        std::process::exit(1);
      }
    }
  };
}
