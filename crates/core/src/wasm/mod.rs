use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use trailbase_auth_config::{AuthConfig, LoginIdentifier, OAuthProvider, RegistrationIdentifier};
use trailbase_reactive::Reactive;
use trailbase_wasm_common::HttpContextUser;
use trailbase_wasm_common::manifest::Metadata;
use trailbase_wasm_runtime_axum::Job;
use utoipa_axum::router::OpenApiRouter;

use crate::config::proto::{Config, UserIdentifier};
use crate::{AppState, DataDir, User};

use trailbase_wasm_runtime_axum::{AnyError, KvStore, Runtime, WasmRuntimeBuilder};
pub use trailbase_wasm_runtime_axum::{
  SqliteFunctions, SqliteStore, build_sync_wasm_runtimes_for_components,
};

type WasmMetadataAndRuntime = (Option<Metadata>, Runtime);

#[derive(Default)]
pub struct WasmState {
  /// Actual WASM runtimes.
  runtimes: Vec<Arc<RwLock<WasmMetadataAndRuntime>>>,
  /// WASM runtime builders needed to rebuild above runtimes, e.g. when hot-reloading.
  builders: Vec<Box<WasmRuntimeBuilder>>,
}

impl WasmState {
  pub fn init(
    data_dir: &DataDir,
    config: Reactive<Config>,
    conn: trailbase_sqlite::Connection,
    tokio_rt: Option<tokio::runtime::Handle>,
    root_fs: Option<PathBuf>,
    dev: bool,
  ) -> Self {
    const AUTH_CONFIG_KEY: &str = "config:auth";
    let shared_kv_store = KvStore::new();
    // Assign right away.
    {
      config.with_value(|c| {
        shared_kv_store.set(
          AUTH_CONFIG_KEY.to_string(),
          serde_json::to_vec(&build_auth_config(c)).expect("startup"),
        );
      });

      // Register an observer for continuous updates.
      let shared_kv_store = shared_kv_store.clone();
      config.add_observer(move |c| {
        if let Ok(v) = serde_json::to_vec(&build_auth_config(c)) {
          shared_kv_store.set(AUTH_CONFIG_KEY.to_string(), v);
        }
      });
    }

    let builders = trailbase_wasm_runtime_axum::wasm_runtime_builders(
      data_dir.root().join("wasm"),
      conn,
      tokio_rt,
      root_fs,
      Some(shared_kv_store),
      dev,
    );

    return Self {
      runtimes: builders
        .iter()
        .map(|builder| Arc::new(RwLock::new((None, builder().expect("startup")))))
        .collect(),
      builders,
    };
  }

  pub fn runtimes(&self) -> &[Arc<RwLock<WasmMetadataAndRuntime>>] {
    return &self.runtimes;
  }

  pub async fn reload_runtimes(&self) -> Result<(), AnyError> {
    let mut new_runtimes = self
      .builders
      .iter()
      .map(|builder| builder())
      .collect::<Result<Vec<_>, _>>()?;
    if new_runtimes.is_empty() {
      return Ok(());
    }

    // TODO: We could also compare manifest of old and new runtime to warn/fail explicitly if
    // routes and or jobs changed.
    log::info!("Reloading WASM components. New HTTP routes and Jobs require a server restart.");

    for old_rt in &self.runtimes {
      let (metadata, component_path) = {
        let old_rt = old_rt.read().await;
        (old_rt.0.clone(), old_rt.1.component_path().to_path_buf())
      };

      let Some(index) = new_runtimes
        .iter()
        .position(|rt| *rt.component_path() == *component_path)
      else {
        log::warn!("WASM component: {component_path:?} was removed. Required server restart");
        continue;
      };

      // Swap out old with new WASM runtime for the given component.
      // TODO: We should probably also update Metadata.
      *old_rt.write().await = (metadata, new_runtimes.remove(index));
    }

    for new_rt in new_runtimes {
      log::warn!(
        "New WASM component found {:?}. Requires server restart.",
        new_rt.component_path()
      );
    }

    return Ok(());
  }
}

#[derive(Default)]
pub(crate) struct InstallResult {
  /// Tuple of router and whether that router has GET "/" route.
  pub router: Option<(OpenApiRouter<AppState>, bool)>,
}

pub(crate) async fn install_routes_and_jobs(
  state: &AppState,
  runtime: Arc<RwLock<(Option<Metadata>, Runtime)>>,
) -> Result<InstallResult, AnyError> {
  use axum::extract::OptionalFromRequestParts;
  use axum::http::request::Parts;

  fn extract_user<'a>(
    parts: &'a mut Parts,
    s: &'a AppState,
  ) -> futures_util::future::BoxFuture<'a, Option<HttpContextUser>> {
    return Box::pin(async {
      User::from_request_parts(parts, s)
        .await
        .ok()
        .flatten()
        .map(|u| HttpContextUser {
          id: u.id,
          email: u.email,
          username: u.username,
          csrf_token: u.csrf_token,
        })
    });
  }

  let version = state.version().git_version_tag.clone();

  let mut metadata_and_rt = runtime.write().await;
  let component_name = metadata_and_rt
    .1
    .component_path()
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or("unknown")
    .to_string();

  let trailbase_wasm_runtime_axum::InstallResult {
    router,
    jobs,
    metadata,
  } = trailbase_wasm_runtime_axum::install_routes_and_jobs::<AppState>(
    &metadata_and_rt.1,
    extract_user,
    version,
  )
  .await?;

  if let Some(metadata) = metadata {
    log::debug!("Registering metadata manifest for WASM component '{component_name}'");
    let _ = metadata_and_rt.0.insert(metadata);
  }

  for Job {
    name,
    schedule,
    callback,
    timeout,
  } in jobs
  {
    let Some(job) = state
      .jobs()
      .new_job(None, name, schedule, timeout, callback)
    else {
      return Err("Failed to add job".into());
    };

    job.start();
  }

  return Ok(InstallResult { router });
}

fn build_auth_config(config: &Config) -> AuthConfig {
  let oauth_providers: Vec<_> = config
    .auth
    .oauth_providers
    .iter()
    .filter_map(|(key, config)| {
      let entry = crate::auth::oauth::providers::oauth_providers_static_registry()
        .iter()
        .find(|registered| config.provider_id == Some(registered.id as i32))?;

      let provider = (entry.factory)(key, config).ok()?;
      let name = provider.name();

      // NOTE: Could instead be a provider trait property.
      fn oauth_provider_name_to_img(name: &str) -> &'static str {
        return match name {
          "discord" => "discord.svg",
          "facebook" => "facebook.svg",
          "github" => "github.svg",
          "gitlab" => "gitlab.svg",
          "google" => "google.svg",
          "microsoft" => "microsoft.svg",
          "twitch" => "twitch.svg",
          "yandex" => "yandex.svg",
          _ => "oidc.svg",
        };
      }

      return Some(OAuthProvider {
        name: name.to_string(),
        display_name: provider.display_name().to_string(),
        img_name: oauth_provider_name_to_img(name).to_string(),
      });
    })
    .collect();

  let user_identifier = config
    .auth
    .user_identifier
    .and_then(|i| i.try_into().ok())
    .unwrap_or(UserIdentifier::Undefined);

  return AuthConfig {
    disable_password_auth: config.auth.disable_password_auth(),
    enable_otp_signin: config.auth.enable_otp_signin(),
    oauth_providers,
    login_identifier: match user_identifier {
      UserIdentifier::OnlyEmail | UserIdentifier::Undefined => LoginIdentifier::OnlyEmail,
      UserIdentifier::OnlyUsername => LoginIdentifier::OnlyUsername,
      _ => LoginIdentifier::EmailOrUsername,
    },
    registration_identifier: match user_identifier {
      UserIdentifier::OnlyEmail | UserIdentifier::Undefined => RegistrationIdentifier::OnlyEmail,
      UserIdentifier::OnlyUsername => RegistrationIdentifier::OnlyUsername,
      UserIdentifier::RequireUsername => RegistrationIdentifier::RequireUsername,
      UserIdentifier::RequireEmail => RegistrationIdentifier::RequireEmail,
      UserIdentifier::RequireEmailAndUsername => RegistrationIdentifier::RequireEmailAndUsername,
    },
  };
}
