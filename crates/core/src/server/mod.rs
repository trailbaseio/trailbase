mod serve;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Extension, Request, State};
use axum::handler::HandlerWithoutStateExt;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::{RequestExt, Router};
use bytes::Bytes;
use http_body_util::BodyExt;
use http_body_util::combinators::UnsyncBoxBody;
use log::*;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, OnceLock};
use tokio::signal;
use tokio::task::JoinSet;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio_rustls::rustls::{ServerConfig, crypto};
use tower::Service;
use tower_cookies::CookieManagerLayer;
use tower_governor::GovernorLayer;
use tower_governor::governor::{GovernorConfig, GovernorConfigBuilder};
use tower_http::services::fs::{ServeDir, ServeFile};
use tower_http::{cors, limit::RequestBodyLimitLayer, trace::TraceLayer};
use tracing_subscriber::{filter, prelude::*};
use trailbase_assets::AssetService;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::admin;
use crate::app_state::{AppState, validate_path};
use crate::auth::util::is_admin;
use crate::auth::{self, AuthError, User};
use crate::connection::ConnectionEntry;
use crate::constants::{ADMIN_API_PATH, AUTH_API_PATH, HEADER_CSRF_TOKEN};
use crate::data_dir::DataDir;
use crate::extract::HasRoot;
use crate::extract::ip::RealIpKeyExtractor;
use crate::init_error::InitError;
use crate::logging;
use crate::records;
use crate::socket_address::SocketAddr;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

/// A set of options to configure serving behaviors. Changing any of these options
/// requires a server restart, which makes them a natural fit for being exposed as command line
/// arguments.
#[derive(Clone, Default, Debug)]
pub struct ServerOptions {
  /// Optional socket address for a dedicated admin HTTP server (UI + API). Similar to address
  /// above.
  pub admin_address: Option<SocketAddr>,

  /// Optional path to static assets that will be served at the HTTP root.
  pub public_dir: Option<PathBuf>,

  /// Enable SPA fallback mode for public_dir.
  pub public_dir_spa: bool,

  /// log an event to stdout.
  pub log_responses: bool,

  /// Limit the set of allowed origins the HTTP server will answer to.
  pub cors_allowed_origins: Vec<String>,

  /// TLS certificate path.
  pub tls_cert: Option<CertificateDer<'static>>,
  /// TLS key path.
  pub tls_key: Option<Arc<PrivateKeyDer<'static>>>,

  /// Custom axum router.
  ///
  /// The `custom_router` will be registered with the http server and `on_first_init` will be
  /// called only when a new data directory and therefore databases are created. This hook can
  /// be used to customize the setup in a simple manner, e.g. create tables, etc.
  /// Note, however, that for a multi-stage deployment (dev, test, staging, prod, ...) or prod
  /// setups migrations are a more robust approach to consistent and continuous management of
  /// schemas.
  pub custom_router: Option<Router<AppState>>,
}

pub struct Server {
  pub state: AppState,

  // Routers.
  pub main_router: (SocketAddr, Router),
  pub admin_router: Option<(SocketAddr, Router)>,

  // TLS/SSL
  pub tls: Option<(CertificateDer<'static>, PrivateKeyDer<'static>)>,
}

impl Server {
  /// Initializes the server. Will create a new data directory on first start.
  ///
  /// Socket `address` the HTTP server binds to, e.g. "localhost:4000" for TCP or "unix:/test" for
  /// a unix-domain-socket (UDS).
  pub async fn init(
    state: AppState,
    address: SocketAddr,
    opts: ServerOptions,
  ) -> Result<Self, InitError> {
    let ServerOptions {
      admin_address,
      public_dir,
      public_dir_spa,
      log_responses,
      cors_allowed_origins,
      tls_cert,
      tls_key,
      custom_router,
    } = opts;

    validate_path(public_dir.as_ref())?;

    let version_info = trailbase_build::get_version_info!();
    info!(
      "Initializing server version: {version} {date}",
      version = version_info.git_version_tag.unwrap_or_default(),
      date = version_info.git_commit_date.unwrap_or_default(),
    );

    Self::build_tracing(&state, log_responses).init();

    let mut custom_routers: Vec<OpenApiRouter<AppState>> =
      custom_router.into_iter().map(|r| r.into()).collect();

    // Whether any of the components provides GET "/" route.
    #[allow(unused_mut)]
    let mut has_root = public_dir.is_some();

    #[cfg(feature = "wasm")]
    {
      for rt in state.wasm().runtimes() {
        if let crate::wasm::InstallResult {
          router: Some((wasm_router, has_wasm_root)),
        } = crate::wasm::install_routes_and_jobs(&state, rt.clone())
          .await
          .map_err(|err| InitError::ScriptError(err.to_string()))?
        {
          has_root |= has_wasm_root;
          custom_routers.push(wasm_router);
        }
      }
    }

    // Install an Ip-based rate limiter *ONLY* for auth APIs to avoid abuse.
    //
    // NOTE: If you run into rate-limits and are running behind a reverse proxy, please set the
    // "x-forwarded-for" header correctly to ensure ip-based rate limiting and request logging
    // works correctly.
    let auth_rate_limit = if !state.dev_mode()
      && let Some(auth_rate_limit) = state.get_config().server.auth_ip_rate_limit
      && auth_rate_limit > 0
    {
      Some(auth_rate_limit)
    } else {
      None
    };

    let independent_admin_router = if let Some(admin_address) = admin_address
      && admin_address != address
    {
      let (router, _api) = OpenApiRouter::new()
        .nest(&format!("/{AUTH_API_PATH}/"), {
          let auth_router = auth::admin_auth_router();
          if let Some(auth_rate_limit) = auth_rate_limit {
            // Limit access to the auth routes only.
            auth_router.layer(GovernorLayer::new(build_shared_governor_conf(
              auth_rate_limit,
            )))
          } else {
            auth_router
          }
        })
        .merge(Self::build_admin_router(&state))
        .split_for_parts();

      // NOTE: For the admin router no (GET, "/") is path installed => has_root=false.
      let admin_router = Self::wrap_with_default_layers(
        &state,
        router,
        &cors_allowed_origins,
        /* has_root= */ false,
      );

      Some((admin_address, admin_router))
    } else {
      // Simply add to the main router.
      custom_routers.push(Self::build_admin_router(&state));
      None
    };

    let (main_router, api) = Self::build_main_router(
      &state,
      public_dir.as_ref(),
      public_dir_spa,
      custom_routers,
      auth_rate_limit,
    )?
    .split_for_parts();

    let main_router =
      Self::wrap_with_default_layers(&state, main_router, &cors_allowed_origins, has_root);
    let api = crate::openapi::add_info(api);
    let tls = load_tls(state.data_dir(), tls_cert, tls_key);

    return if let Some((admin_address, admin_router)) = independent_admin_router {
      Ok(Self {
        state,
        main_router: (address, main_router),
        admin_router: Some((admin_address, admin_router.layer(Extension(api)))),
        tls,
      })
    } else {
      Ok(Self {
        state,
        main_router: (address, main_router.layer(Extension(api))),
        admin_router: None,
        tls,
      })
    };
  }

  fn build_tracing(
    state: &AppState,
    log_responses: bool,
  ) -> impl tracing_subscriber::layer::SubscriberExt {
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
    #[cfg(not(feature = "otel"))]
    let subscriber = tracing_subscriber::Registry::default();

    #[cfg(feature = "otel")]
    let subscriber = {
      let (subscriber, otel_guard) =
        init_tracing_opentelemetry::tracing_subscriber_ext::regiter_otel_layers(
          tracing_subscriber::Registry::default(),
        )
        .expect("startup");

      // TODO: We have to keep this alive. Let's find something better than a singleton.
      static SINGLETON: OnceLock<init_tracing_opentelemetry::Guard> = OnceLock::new();
      SINGLETON.get_or_init(move || init_tracing_opentelemetry::Guard::global(Some(otel_guard)));

      subscriber
    };

    let filter_layer = filter::Targets::new()
      .with_default(filter::LevelFilter::OFF)
      .with_target(crate::logging::EVENT_TARGET, crate::logging::LEVEL);

    return subscriber
      .with(filter_layer)
      .with(logging::SqliteLogLayer::new(
        state,
        /* log-to-stdout= */ log_responses,
      ));
  }

  pub async fn serve(self) -> Result<(), AnyError> {
    // Make sure TLS provider is installed. Required for both incoming and outgoing traffic,
    // including traffic from WASM components, e.g. `fetch("https://example.com")`.
    if crypto::CryptoProvider::get_default().is_none() {
      info!("No process-wide TLS provider found. Falling back to `aws_lc_rs`.");
      if let Err(_provider) = crypto::aws_lc_rs::default_provider().install_default() {
        // QUESTION: Should this be a panic or is this still acceptable for users who don't
        // need TLS (neither to serve nor for WASM components).
        error!("Installing fallback TLS provider failed.");
      }
    }

    // Install a SIGHUP/hangup signal handler to reload config and WASM runtimes (in dev mode).
    #[cfg(unix)]
    start_sighup_reload_task(self.state.clone());

    // Graceful shutdown is handled by axum (this is independent from SIGHUP handler above).
    let cleanup_sender = {
      let (cleanup_sender, cleanup_receiver) = tokio::sync::oneshot::channel::<()>();

      tokio::spawn(async move {
        if cleanup_receiver.await.is_ok() {
          log::debug!("cleanup started");

          // Shutdown established subscriptions streams.
          self.state.subscription_manager().shutdown();

          // NOTE: Disabled since prost-reflect prints map entries in random order (uses
          // HashMap internally).
          //
          // Write the latest config state back to disk. Right now we only do this in debug builds
          // to make sure our checked-in configurations are stable and up-to-date.
          // #[cfg(debug_assertions)]
          // if let Err(err) = self
          //   .state
          //   .validate_and_update_config((*self.state.get_config()).clone(), None)
          //   .await
          // {
          //   panic!("Failed to write configs: {err}");
          // }
        }
      });

      cleanup_sender
    };

    // Finally start to listen.
    {
      let protocol = if self.tls.is_some() { "https" } else { "http" };
      let (is_uds, base_uri) = match self.main_router.0 {
        SocketAddr::Uds(ref path) => (true, format!("unix:{}", path.to_string_lossy())),
        SocketAddr::Tcp(ref a) => (false, format!("{protocol}://{a}")),
      };
      let mut admin_uri: Option<String> = None;

      let mut set = JoinSet::new();

      if let Some((admin_addr, admin_router)) = self.admin_router {
        if let SocketAddr::Tcp(aa) = admin_addr {
          admin_uri = Some(format!("{protocol}://{aa}/_/admin/"));
        }

        let cloned_tls = self
          .tls
          .as_ref()
          .map(|(cert, key)| (cert.clone(), key.clone_key()));

        set.spawn(async move { start_listen(admin_addr, admin_router, cloned_tls, None).await });
      } else if is_uds {
        admin_uri = Some(format!("{base_uri}/_/admin/"));
      }

      set.spawn({
        let (addr, router) = self.main_router;
        async move { start_listen(addr, router, self.tls, Some(cleanup_sender)).await }
      });

      if let Some(admin_uri) = admin_uri {
        info!("Listening on {base_uri} 🚀 (Admin UI: {admin_uri})");
      } else {
        info!("Listening on {base_uri} 🚀");
      }

      set.join_all().await;
    }

    println!("Shut down gracefully 👋");

    return Ok(());
  }

  pub(crate) fn build_admin_router(state: &AppState) -> OpenApiRouter<AppState> {
    return OpenApiRouter::new()
      .nest(
        &format!("/{ADMIN_API_PATH}/"),
        admin::router().layer(middleware::from_fn_with_state(
          state.clone(),
          assert_admin_api_access,
        )),
      )
      // NOTE: We cannot ACL-lock the UI assets. We need to be able to sign into the SPA.
      .nest_service(
        "/_/admin",
        AssetService::<trailbase_assets::AdminAssets>::with_parameters(
          |_path: &str| -> Option<Response<Body>> {
            // SPA fallback.
            let file = trailbase_assets::AdminAssets::get("index.html")?;

            return Some(
              Response::builder()
                .header(axum::http::header::CONTENT_TYPE, file.metadata.mimetype())
                .body(Body::from(cow_to_bytes(file.data)))
                .unwrap_or_default(),
            );
          },
        ),
      );
  }

  pub(crate) fn build_main_router(
    state: &AppState,
    public_dir: Option<&PathBuf>,
    public_dir_spa: bool,
    custom_routers: Vec<OpenApiRouter<AppState>>,
    auth_rate_limit: Option<u32>,
  ) -> Result<OpenApiRouter<AppState>, InitError> {
    let enable_transactions =
      state.access_config(|conn| conn.server.enable_record_transactions.unwrap_or(false));

    let ConnectionEntry {
      connection: conn, ..
    } = state.connection_manager().main_entry();

    let mut router = OpenApiRouter::new()
      // Public, stable and versioned APIs.
      .merge(records::router(conn.connection_type(), enable_transactions))
      .nest(&format!("/{AUTH_API_PATH}/"), {
        let auth_router = auth::router(&state.get_config());
        if let Some(auth_rate_limit) = auth_rate_limit {
          auth_router.layer(GovernorLayer::new(build_shared_governor_conf(
            auth_rate_limit,
          )))
        } else {
          auth_router
        }
      })
      .routes(routes!(healthcheck_handler));

    #[cfg(debug_assertions)]
    {
      use crate::auth::user::User;

      #[utoipa::path(
          get,
          path = "/api/whoami",
          tag = "status",
          responses((status = 200, description = "Success", body = String))
      )]
      pub async fn whoami_handler(user: Option<User>) -> String {
        return format!("{user:?}");
      }

      router = router.routes(routes!(whoami_handler));
    }

    for custom_router in custom_routers {
      router = router.merge(custom_router);
    }

    if let Some(public_dir) = public_dir {
      if !std::fs::exists(public_dir).unwrap_or(false) {
        panic!("--public_dir={public_dir:?} path does not exist.")
      }

      const NOT_FOUND: &[u8] = b"Not found";
      async fn handle_404() -> (StatusCode, &'static [u8]) {
        (StatusCode::NOT_FOUND, NOT_FOUND)
      }

      router = if public_dir_spa {
        let spa_fallback = public_dir.join("index.html");
        if std::fs::exists(&spa_fallback).unwrap_or(false) {
          let mut index_file = ServeFile::new(spa_fallback);
          let fallback = async move |req: Request| {
            static SUFFIX_RE: LazyLock<regex::Regex> =
              LazyLock::new(|| regex::Regex::new(r"[.]\w+$").expect("const"));

            // Return NOT_FOUND when the requested path ends in a suffix. Not sure if this is ideal
            // but we definitely don't want to return an "index.html" on a favicon request.
            if SUFFIX_RE.is_match(req.uri().path()) {
              return Ok(
                Response::builder()
                  .status(StatusCode::NOT_FOUND)
                  .body(axum::body::Body::from(NOT_FOUND).boxed_unsync())
                  .expect("infallible"),
              );
            }

            return index_file.call(req).await.map(|response| {
              response.map(|body| UnsyncBoxBody::new(body.map_err(axum::Error::new)))
            });
          };

          router.fallback_service(ServeDir::new(public_dir).fallback(fallback.into_service()))
        } else {
          warn!("--spa specified but index.html not found");
          router.fallback_service(ServeDir::new(public_dir).fallback(handle_404.into_service()))
        }
      } else {
        router
          .fallback_service(ServeDir::new(public_dir).not_found_service(handle_404.into_service()))
      };
    }

    return Ok(router);
  }

  pub fn wrap_with_default_layers(
    state: &AppState,
    router: Router<AppState>,
    cors_allowed_origins: &[String],
    has_root: bool,
  ) -> Router<()> {
    #[cfg(feature = "otel")]
    let router = router
      .layer(axum_tracing_opentelemetry::middleware::OtelInResponseLayer)
      .layer(axum_tracing_opentelemetry::middleware::OtelAxumLayer::default());

    return router
      .layer(Extension(HasRoot(has_root)))
      .layer(CookieManagerLayer::new())
      .layer(build_cors(cors_allowed_origins, state.dev_mode()))
      .layer(
        // This declares: **what information** is logged at what level in to events and spans.
        TraceLayer::new_for_http()
          .make_span_with(logging::sqlite_logger_make_span)
          .on_request(logging::sqlite_logger_on_request)
          .on_response(logging::sqlite_logger_on_response),
      )
      // Default request size limit is only 2MB Increase to 10MB by default if no explicit user
      // limit is provided.
      .layer(DefaultBodyLimit::disable())
      .layer(RequestBodyLimitLayer::new(
        state
          .get_config()
          .server
          .request_size_limit_bytes
          .map_or(10 * 1024 * 1024, |limit| limit as usize),
      ))
      .with_state(state.clone());
  }
}

#[utoipa::path(
    get,
    path = "/api/healthcheck",
    tag = "status",
    responses((status = 200, description = "Success", body = String))
)]
async fn healthcheck_handler() -> &'static str {
  return "Ok";
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

  // IMPORTANT: We cannot trust the admin bit in the auth-token, since it may be stale. We need to
  // query the DB.
  if !is_admin(&state, &user.uuid).await {
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

fn build_cors(cors_allowed_origins: &[String], dev: bool) -> cors::CorsLayer {
  if dev {
    return cors::CorsLayer::very_permissive();
  }

  let wildcard = cors_allowed_origins.iter().any(|s| s == "*");

  let origins = if wildcard {
    info!("CORS: allow any origin");
    // cors::AllowOrigin::any()
    cors::AllowOrigin::mirror_request()
  } else {
    cors::AllowOrigin::list(cors_allowed_origins.iter().filter_map(
      |o| match HeaderValue::from_str(o.as_str()) {
        Ok(value) => Some(value),
        Err(err) => {
          error!("Invalid CORS origin {o}: {err}");
          None
        }
      },
    ))
  };

  // Cannot combine `Access-Control-Allow-Credentials: true` with `Access-Control-Allow-Methods: *`
  //
  // We cannot further limit the set of allowed methods or headers with routes potentially being
  // provided by WASM components.
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

  fn start_shutdown_timer(handle: tokio::runtime::Handle) {
    std::thread::spawn(move || {
      let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("We're shutting down and failed to start the watch timer - might as well panic.");

      rt.block_on(async {
        use tokio::time::*;

        log::debug!(
          "Shutdown watchdog started. Pending tasks: {}",
          handle.metrics().num_alive_tasks()
        );

        const SECONDS: usize = 10;

        for remaining in (0..SECONDS).rev() {
          tokio::select! {
            _ = sleep(Duration::from_secs(1)) => {}
            _ = signal::ctrl_c() => {
                println!("Got another Ctrl+C => force shutdown");
                std::process::exit(1);
            }
          };

          if remaining > 0 {
            println!(
              "Waiting {SECONDS}s for graceful shutdown (pending: {}): {remaining}s remaining.",
              handle.metrics().num_alive_tasks()
            );
          } else {
            println!("Graceful shutdown failed. Shutting down");
            std::process::exit(0);
          }
        }
      })
    });
  }

  let rt = tokio::runtime::Handle::current();

  // We're spawning a timer. We *must* not await it. Otherwise we're holding up the shut-down.
  tokio::select! {
      _ = ctrl_c => {
      println!("Received Ctrl+C. Shutting down gracefully.");
      start_shutdown_timer(rt);
    },
      _ = terminate => {
      println!("Received termination. Shutting down gracefully.");
      start_shutdown_timer(rt);
    },
  };

  // Ready to shut down.
}

async fn start_listen(
  addr: SocketAddr,
  router: Router<()>,
  tls: Option<(CertificateDer<'static>, PrivateKeyDer<'static>)>,
  cleanup_sender: Option<tokio::sync::oneshot::Sender<()>>,
) {
  fn listen_err(err: std::io::Error) {
    error!("Failed to listen: {err}");
    std::process::exit(1);
  }

  let graceful_shutdown = async {
    shutdown_signal().await;

    if let Some(cleanup) = cleanup_sender {
      let _ = cleanup.send(());
    }
  };

  let result = match (addr, tls) {
    (SocketAddr::Uds(_), Some(_)) => {
      error!("TLS + UDS not supported");
      std::process::exit(1);
    }
    (SocketAddr::Uds(path), None) => {
      #[cfg(unix)]
      {
        let listener: tokio::net::UnixListener = tokio::net::UnixListener::bind(&path)
          .map_err(listen_err)
          .expect("terminate");

        serve::serve(
          listener,
          router.into_make_service_with_connect_info::<tokio::net::unix::SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
          graceful_shutdown.await;

          // Delete the socket.
          let _ = std::fs::remove_file(path);
        })
        .await
      }

      #[cfg(not(unix))]
      panic!("UDS not supported on Windows")
    }
    (SocketAddr::Tcp(a), None) => {
      let listener = tokio::net::TcpListener::bind(a)
        .await
        .map_err(listen_err)
        .expect("terminate");

      serve::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
      )
      .with_graceful_shutdown(graceful_shutdown)
      .await
    }
    (SocketAddr::Tcp(a), Some((cert, key))) => {
      let listener = tokio::net::TcpListener::bind(a)
        .await
        .map_err(listen_err)
        .expect("terminate");

      serve::serve(
        serve::TlsListener {
          listener,
          acceptor: TlsAcceptor::from(Arc::new({
            ServerConfig::builder()
              .with_no_client_auth()
              .with_single_cert(vec![cert], key)
              .expect("Failed to build server config")
          })),
        },
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
      )
      .with_graceful_shutdown(graceful_shutdown)
      .await
    }
  };

  if let Err(err) = result {
    error!("Failed to start server: {err}");
    std::process::exit(1);
  }
}

fn cow_to_bytes(cow: Cow<'static, [u8]>) -> Bytes {
  match cow {
    Cow::Borrowed(x) => Bytes::from(x),
    Cow::Owned(x) => Bytes::from(x),
  }
}

fn load_tls(
  data_dir: &DataDir,
  tls_cert: Option<CertificateDer<'static>>,
  tls_key: Option<Arc<PrivateKeyDer<'static>>>,
) -> Option<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
  let tls_cert = tls_cert.map_or_else(
    || {
      std::fs::read(data_dir.secrets_path().join("certs").join("cert.pem"))
        .ok()
        .and_then(|cert| CertificateDer::from_pem_slice(&cert).ok())
    },
    Some,
  );
  let tls_key = tls_key.map_or_else(
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

type Governor =
  GovernorConfig<RealIpKeyExtractor, governor::middleware::StateInformationMiddleware>;

fn build_shared_governor_conf(rate_limit: u32) -> Arc<Governor> {
  static GOVERNOR_CONF: OnceLock<Arc<Governor>> = OnceLock::new();

  let governor_conf = GOVERNOR_CONF.get_or_init(|| {
    let governor_conf = Arc::new(
      GovernorConfigBuilder::default()
        // Quota.
        .burst_size(rate_limit)
        // Replenish one after 1 seconds.
        .per_second(1)
        .key_extractor(RealIpKeyExtractor)
        // Set rate limiting headers on reply.
        .use_headers()
        // Only block POST method for abuse prevention (e.g. sign-up, ...), e.g. allow unlimited
        // GET auth status.
        .methods(vec![axum::http::Method::POST])
        .finish()
        .expect("startup"),
    );

    // Periodically clean up governor.
    tokio::spawn({
      let governor_limiter = governor_conf.limiter().clone();
      async move {
        let interval = tokio::time::Duration::from_secs(60);
        loop {
          tokio::time::sleep(interval).await;
          log::trace!("rate limiting storage size: {}", governor_limiter.len());
          governor_limiter.retain_recent();
        }
      }
    });

    return governor_conf;
  });

  return governor_conf.clone();
}

#[cfg(unix)]
fn start_sighup_reload_task(state: AppState) {
  // An infinite stream of hangup signals.
  let mut stream = signal::unix::signal(signal::unix::SignalKind::hangup()).expect("startup");

  tokio::spawn(async move {
    loop {
      stream.recv().await;

      info!(
        "Received SIGHUP: reloading WASM components (dev), re-apply db migrations, and finally re-load config."
      );

      #[cfg(feature = "wasm")]
      if state.dev_mode()
        && let Err(err) = state.wasm().reload_runtimes().await
      {
        warn!("Reloading WASM failed: {err}");
      }

      // Re-apply migrations. This needs to happen before reloading the config, which is
      // consistent with the startup order. Otherwise, we may validate a configuration
      // against a stale database schema.
      //
      // TODO: Right now we're only re-applying main migrations.
      let user_migrations_path = state.data_dir().migrations_path();
      let conn = state.connection_manager().main_entry().connection;

      match crate::migrations::apply_main_migrations(&conn, Some(user_migrations_path))
        .await
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()))
      {
        Err(err) => {
          // NOTE: it's not clear what the best error behavior here is. Should the server
          // continue to run when migrations fail?
          error!("Failed to apply migrations: {err}");
        }
        Ok(_new_db) => {
          let user_migrations_path = state.data_dir().migrations_path();
          info!("Migrations applied: {user_migrations_path:?}");
        }
      }

      // NOTE: we're always invalidating: simple & safe. We could also avoid invalidation
      // when no new migrations were applied :shrug:.
      if let Err(err) = state.rebuild_connection_metadata().await {
        error!("Failed to invalidate schema cache: {err}");
      }

      // Reload config:
      match crate::config::load_or_init_config_textproto(
        state.data_dir(),
        &state.connection_manager(),
      )
      .await
      {
        Ok(config) => {
          if let Err(err) = state.validate_and_update_config(config, None).await {
            error!("Failed to reload config: {err}");
          }
        }
        Err(err) => {
          error!("Failed to reload config: {err}");
        }
      }
    }
  });
}
