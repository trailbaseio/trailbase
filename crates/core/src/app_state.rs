use log::*;
use object_store::ObjectStore;
use reactivate::{Merge, Reactive};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use trailbase_extension::jsonschema::JsonSchemaRegistry;
use trailbase_schema::QualifiedName;

use crate::auth::jwt::JwtHelper;
use crate::auth::options::AuthOptions;
use crate::config::proto::{Config, RecordApiConfig, S3StorageConfig, hash_config};
use crate::config::{ConfigError, validate_config, write_config_and_vault_textproto};
use crate::data_dir::DataDir;
use crate::email::Mailer;
use crate::records::RecordApi;
use crate::records::subscribe::SubscriptionManager;
use crate::scheduler::{JobRegistry, build_job_registry_from_config};
use crate::schema_metadata::{
  ConnectionMetadata, build_connection_metadata_and_install_file_deletion_triggers,
  lookup_and_parse_all_table_schemas, lookup_and_parse_all_view_schemas,
};
use crate::wasm::Runtime;

/// The app's internal state. AppState needs to be clonable which puts unnecessary constraints on
/// the internals. Thus rather arc once than many times.
struct InternalState {
  data_dir: DataDir,
  public_dir: Option<PathBuf>,
  runtime_root_fs: Option<PathBuf>,

  site_url: Reactive<Arc<Option<url::Url>>>,
  dev: bool,
  demo: bool,

  auth: Reactive<Arc<AuthOptions>>,
  jobs: Reactive<Arc<JobRegistry>>,
  mailer: Reactive<Mailer>,
  record_apis: Reactive<Arc<Vec<(String, RecordApi)>>>,
  config: Reactive<Config>,
  json_schema_registry: Arc<parking_lot::RwLock<JsonSchemaRegistry>>,

  conn: trailbase_sqlite::Connection,
  logs_conn: trailbase_sqlite::Connection,

  jwt: JwtHelper,

  connection_metadata: Reactive<Arc<ConnectionMetadata>>,
  subscription_manager: SubscriptionManager,
  object_store: Arc<dyn ObjectStore + Send + Sync>,

  /// Actual WASM runtimes.
  wasm_runtimes: Vec<Arc<RwLock<Runtime>>>,
  /// WASM runtime builders needed to rebuild above runtimes, e.g. when hot-reloading.
  build_wasm_runtimes: Box<dyn Fn() -> Result<Vec<Runtime>, crate::wasm::AnyError> + Send + Sync>,

  #[cfg(test)]
  #[allow(unused)]
  test_cleanup: Vec<Box<dyn std::any::Any + Send + Sync>>,
}

pub(crate) struct AppStateArgs {
  pub data_dir: DataDir,
  pub public_url: Option<url::Url>,
  pub public_dir: Option<PathBuf>,
  pub runtime_root_fs: Option<PathBuf>,
  pub dev: bool,
  pub demo: bool,
  pub connection_metadata: ConnectionMetadata,
  pub config: Config,
  pub json_schema_registry: Arc<parking_lot::RwLock<JsonSchemaRegistry>>,
  pub conn: trailbase_sqlite::Connection,
  pub logs_conn: trailbase_sqlite::Connection,
  pub jwt: JwtHelper,
  pub object_store: Box<dyn ObjectStore + Send + Sync>,
  pub runtime_threads: Option<usize>,
}

#[derive(Clone)]
pub struct AppState {
  state: Arc<InternalState>,
}

impl AppState {
  pub(crate) fn new(args: AppStateArgs) -> Self {
    let config = Reactive::new(args.config);

    let public_url = args.public_url.clone();
    let site_url = config.derive(move |config| {
      if let Some(ref public_url) = public_url {
        debug!("Public url provided: {public_url:?}");
        return Arc::new(Some(public_url.clone()));
      }

      return Arc::new(
        build_site_url(config)
          .map_err(|err| {
            error!("Failed to parse `site_url`: {err}");
            return err;
          })
          .ok()
          .flatten(),
      );
    });

    let connection_metadata = Reactive::new(Arc::new(args.connection_metadata));
    let record_apis = {
      let conn = args.conn.clone();
      let m = (&config, &connection_metadata).merge();

      derive_unchecked(&m, move |(config, metadata)| {
        debug!("(re-)building Record APIs");

        return Arc::new(
          config
            .record_apis
            .iter()
            .filter_map(
              |config| match build_record_api(conn.clone(), metadata, config.clone()) {
                Ok(api) => Some((api.api_name().to_string(), api)),
                Err(err) => {
                  error!("{err}");
                  None
                }
              },
            )
            .collect::<Vec<_>>(),
        );
      })
    };

    let object_store: Arc<dyn ObjectStore + Send + Sync> = args.object_store.into();
    let jobs_input = (
      args.data_dir.clone(),
      args.conn.clone(),
      args.logs_conn.clone(),
      object_store.clone(),
    );

    let crate::wasm::WasmRuntimeResult {
      shared_kv_store,
      build_wasm_runtime,
    } = crate::wasm::build_wasm_runtime(
      args.data_dir.clone(),
      args.conn.clone(),
      args.runtime_root_fs.clone(),
      args.runtime_threads,
      args.dev,
    )
    .expect("startup");

    // Assign right away.
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

    AppState {
      state: Arc::new(InternalState {
        data_dir: args.data_dir,
        public_dir: args.public_dir,
        runtime_root_fs: args.runtime_root_fs,
        site_url,
        dev: args.dev,
        demo: args.demo,
        auth: derive_unchecked(&config, |c| {
          Arc::new(AuthOptions::from_config(c.auth.clone()))
        }),
        jobs: derive_unchecked(&config, move |c| {
          debug!("(re-)building jobs from config");

          let (data_dir, conn, logs_conn, object_store) = &jobs_input;

          return Arc::new(
            build_job_registry_from_config(c, data_dir, conn, logs_conn, object_store.clone())
              .unwrap_or_else(|err| {
                error!("Failed to build JobRegistry for cron jobs: {err}");
                return JobRegistry::new();
              }),
          );
        }),
        mailer: derive_unchecked(&config, Mailer::new_from_config),
        record_apis: record_apis.clone(),
        config,
        json_schema_registry: args.json_schema_registry,
        conn: args.conn.clone(),
        logs_conn: args.logs_conn,
        jwt: args.jwt,
        subscription_manager: SubscriptionManager::new(
          args.conn.clone(),
          connection_metadata.clone(),
          record_apis,
        ),
        connection_metadata,
        object_store,
        wasm_runtimes: build_wasm_runtime()
          .expect("startup")
          .into_iter()
          .map(|rt| Arc::new(RwLock::new(rt)))
          .collect(),
        build_wasm_runtimes: build_wasm_runtime,
        #[cfg(test)]
        test_cleanup: vec![],
      }),
    }
  }

  /// Path where TrailBase stores its data, config, migrations, and secrets.
  pub fn data_dir(&self) -> &DataDir {
    return &self.state.data_dir;
  }

  /// Optional user-prvoided public directory from where static assets are served.
  pub fn public_dir(&self) -> Option<&Path> {
    return self.state.public_dir.as_deref();
  }

  /// Optional user-prvoided public directory from where static assets are served.
  pub fn runtime_root_fs(&self) -> Option<&Path> {
    return self.state.runtime_root_fs.as_deref();
  }

  pub(crate) fn dev_mode(&self) -> bool {
    return self.state.dev;
  }

  pub(crate) fn demo_mode(&self) -> bool {
    return self.state.demo;
  }

  pub fn json_schema_registry(&self) -> &Arc<parking_lot::RwLock<JsonSchemaRegistry>> {
    return &self.state.json_schema_registry;
  }

  pub fn conn(&self) -> &trailbase_sqlite::Connection {
    return &self.state.conn;
  }

  pub fn user_conn(&self) -> &trailbase_sqlite::Connection {
    return &self.state.conn;
  }

  pub fn logs_conn(&self) -> &trailbase_sqlite::Connection {
    return &self.state.logs_conn;
  }

  pub fn version(&self) -> trailbase_build::version::VersionInfo {
    return trailbase_build::get_version_info!();
  }

  pub(crate) fn connection_metadata(&self) -> Arc<ConnectionMetadata> {
    return self.state.connection_metadata.value();
  }

  pub(crate) fn subscription_manager(&self) -> &SubscriptionManager {
    return &self.state.subscription_manager;
  }

  pub async fn rebuild_connection_metadata(
    &self,
  ) -> Result<(), crate::schema_metadata::SchemaLookupError> {
    let tables = lookup_and_parse_all_table_schemas(self.conn()).await?;
    let views = lookup_and_parse_all_view_schemas(self.conn(), &tables).await?;

    // We typically rebuild the schema representations when the DB schemas change, which in turn
    // can invalidate the config, e.g. an API may reference a deleted table. Let's make sure to
    // check. Note however that this is tricky to deal with, since the schema changes have already
    // happened rendering the current config invalid. Unlike a config update, it's too late to
    // reject anything.
    let config = self.get_config();
    validate_config(&tables, &views, &config).map_err(|err| {
      log::error!("Schema change invalidated config: {err}");
      return crate::schema_metadata::SchemaLookupError::Other(err.into());
    })?;

    let connection_metadata = build_connection_metadata_and_install_file_deletion_triggers(
      self.conn(),
      tables,
      views,
      &self.state.json_schema_registry,
    )
    .await?;

    self
      .state
      .connection_metadata
      .set(Arc::new(connection_metadata));

    return Ok(());
  }

  pub(crate) fn objectstore(&self) -> &(dyn ObjectStore + Send + Sync) {
    return &*self.state.object_store;
  }

  pub(crate) fn jobs(&self) -> Arc<JobRegistry> {
    return self.state.jobs.value();
  }

  pub(crate) fn auth_options(&self) -> Arc<AuthOptions> {
    return self.state.auth.value();
  }

  pub fn site_url(&self) -> Arc<Option<url::Url>> {
    return self.state.site_url.value();
  }

  pub(crate) fn mailer(&self) -> Mailer {
    return self.state.mailer.value();
  }

  pub(crate) fn jwt(&self) -> &JwtHelper {
    return &self.state.jwt;
  }

  pub fn lookup_record_api(&self, name: &str) -> Option<RecordApi> {
    for (record_api_name, record_api) in &*self.state.record_apis.value() {
      if record_api_name == name {
        return Some(record_api.clone());
      }
    }
    return None;
  }

  pub fn get_config(&self) -> Config {
    return self.state.config.value();
  }

  pub fn access_config<F, T>(&self, f: F) -> T
  where
    F: FnOnce(&Config) -> T,
  {
    let mut result: Option<T> = None;
    let r = &mut result;
    self.state.config.with_value(move |c| {
      let _ = r.insert(f(c));
    });
    return result.expect("inserted");
  }

  pub async fn validate_and_update_config(
    &self,
    config: Config,
    hash: Option<String>,
  ) -> Result<(), ConfigError> {
    let metadata = self.connection_metadata();
    validate_config(&metadata.tables(), &metadata.views(), &config)?;

    match hash {
      Some(hash) => {
        let mut error: Option<ConfigError> = None;
        let err = &mut error;
        self.state.config.update(move |old| {
          if hash_config(old) != hash {
            let _ = err.insert(ConfigError::Update(
              "Config update failed: mismatching or stale hash".to_string(),
            ));

            return old.clone();
          }

          return config;
        });

        if let Some(err) = error {
          return Err(err);
        }
      }
      None => {
        self.state.config.set(config);
      }
    };

    let new_config = self.get_config();

    {
      if update_json_schema_registry(&new_config, &self.state.json_schema_registry).unwrap_or(true)
        && let Err(err) = self.rebuild_connection_metadata().await
      {
        log::warn!("reloading JSON schema cache failed: {err}");
      }
    }

    // Write new config to the file system.
    let metadata = self.connection_metadata();
    return write_config_and_vault_textproto(
      self.data_dir(),
      &metadata.tables(),
      &metadata.views(),
      &new_config,
    )
    .await;
  }

  pub(crate) fn wasm_runtimes(&self) -> &[Arc<RwLock<Runtime>>] {
    return &self.state.wasm_runtimes;
  }

  pub(crate) async fn reload_wasm_runtimes(&self) -> Result<(), crate::wasm::AnyError> {
    let mut new_runtimes = (self.state.build_wasm_runtimes)()?;
    if new_runtimes.is_empty() {
      return Ok(());
    }

    // TODO: Differentiate between an actual rebuild vs a cached re-build to warn users
    // about routes not being able to be changed.
    info!("Reloading WASM components. New HTTP routes and Jobs require a server restart.");

    for old_rt in &self.state.wasm_runtimes {
      let component_path = old_rt.read().await.component_path().clone();

      let Some(index) = new_runtimes
        .iter()
        .position(|rt| *rt.component_path() == component_path)
      else {
        warn!("WASM component: {component_path:?} was removed. Required server restart");
        continue;
      };

      // Swap out old with new WASM runtime for the given component.
      *old_rt.write().await = new_runtimes.remove(index);
    }

    for new_rt in new_runtimes {
      warn!(
        "New WASM component found {:?}. Requires server restart.",
        new_rt.component_path()
      );
    }

    return Ok(());
  }
}

/// Returns true if schemas were registered.
pub(crate) fn update_json_schema_registry(
  config: &Config,
  registry: &parking_lot::RwLock<JsonSchemaRegistry>,
) -> Result<bool, ConfigError> {
  if !config.schemas.is_empty() {
    let schemas: Vec<_> = config
      .schemas
      .iter()
      .map(|s| {
        // Any panics here should be captured by config validation during load above.
        let (Some(name), Some(schema)) = (&s.name, &s.schema) else {
          return Err(ConfigError::Invalid(format!(
            "Schema config invalid entry: {s:?}"
          )));
        };

        let schema_json = serde_json::from_str(schema).map_err(|err| {
          return ConfigError::Invalid(format!("Invalid schema definition for '{name}': {err}"));
        })?;

        return Ok((name.clone(), schema_json));
      })
      .collect::<Result<Vec<_>, _>>()?;

    debug!(
      "Initializing JSON schemas from config: {schemas:?}",
      schemas = schemas.iter().map(|(name, _)| name.as_str())
    );

    registry.write().swap(
      trailbase_schema::registry::build_json_schema_registry(schemas).map_err(|err| {
        return ConfigError::Update(format!("Update of JSON schema registry failed: {err}"));
      })?,
    );

    return Ok(true);
  }

  return Ok(false);
}

/// Construct a fabricated config for tests and make sure it's valid.
#[cfg(test)]
pub fn test_config() -> Config {
  use crate::auth::oauth::providers::test::TestOAuthProvider;
  use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

  let mut config = Config::new_with_custom_defaults();

  config.server.site_url = Some("https://test.org".to_string());
  config.email.smtp_host = Some("smtp.test.org".to_string());
  config.email.smtp_port = Some(587);
  config.email.smtp_username = Some("user".to_string());
  config.email.smtp_password = Some("pass".to_string());
  config.email.sender_address = Some("sender@test.org".to_string());
  config.email.sender_name = Some("Mia Sender".to_string());

  config.auth.oauth_providers.insert(
    TestOAuthProvider::NAME.to_string(),
    OAuthProviderConfig {
      client_id: Some("test_client_id".to_string()),
      client_secret: Some("test_client_secret".to_string()),
      provider_id: Some(OAuthProviderId::Test as i32),
      ..Default::default()
    },
  );
  config
    .auth
    .custom_uri_schemes
    .push("test-scheme".to_string());

  return config;
}

#[cfg(test)]
#[derive(Default)]
pub struct TestStateOptions {
  pub config: Option<Config>,
  pub json_schema_registry: Option<JsonSchemaRegistry>,
  pub(crate) mailer: Option<Mailer>,
}

#[cfg(test)]
pub async fn test_state(options: Option<TestStateOptions>) -> anyhow::Result<AppState> {
  use reactivate::Merge;

  let _ = env_logger::try_init_from_env(
    env_logger::Env::new().default_filter_or("info,trailbase_refinery=warn,log::span=warn"),
  );

  let temp_dir = temp_dir::TempDir::new()?;
  tokio::fs::create_dir_all(temp_dir.child("uploads")).await?;

  let TestStateOptions {
    config,
    mailer,
    json_schema_registry,
  } = options.unwrap_or_default();

  let json_schema_registry = Arc::new(parking_lot::RwLock::new(
    json_schema_registry
      .unwrap_or_else(|| trailbase_schema::registry::build_json_schema_registry(vec![]).unwrap()),
  ));
  let (conn, new) =
    crate::connection::init_main_db(None, Some(json_schema_registry.clone()), vec![], vec![])?;
  assert!(new);

  let logs_conn = crate::connection::init_logs_db(None)?;

  let tables = lookup_and_parse_all_table_schemas(&conn).await?;
  let views = lookup_and_parse_all_view_schemas(&conn, &tables).await?;

  let config = {
    let config = config.unwrap_or_else(test_config);
    validate_config(&tables, &views, &config).unwrap();
    update_json_schema_registry(&config, &json_schema_registry).unwrap();

    Reactive::new(config)
  };

  let connection_metadata = build_connection_metadata_and_install_file_deletion_triggers(
    &conn,
    tables,
    views,
    &json_schema_registry,
  )
  .await?;

  let data_dir = DataDir(temp_dir.path().to_path_buf());

  let object_store = if std::env::var("TEST_S3_OBJECT_STORE").map_or(false, |v| v == "TRUE") {
    info!("Use S3 Storage for tests");

    build_objectstore(
      &data_dir,
      Some(&S3StorageConfig {
        endpoint: Some("http://127.0.0.1:9000".to_string()),
        region: None,
        bucket_name: Some("test".to_string()),
        access_key: Some("minioadmin".to_string()),
        secret_access_key: Some("minioadmin".to_string()),
      }),
    )
    .unwrap()
    .into()
  } else {
    build_objectstore(&data_dir, None).unwrap().into()
  };

  let connection_metadata = Reactive::new(Arc::new(connection_metadata));
  let record_apis: Reactive<Arc<Vec<(String, RecordApi)>>> = {
    let conn = conn.clone();
    let m = (&config, &connection_metadata).merge();

    derive_unchecked(&m, move |(c, metadata)| {
      return Arc::new(
        c.record_apis
          .iter()
          .map(|config| {
            let api = build_record_api(conn.clone(), &metadata, config.clone()).unwrap();
            return (api.api_name().to_string(), api);
          })
          .collect::<Vec<_>>(),
      );
    })
  };

  return Ok(AppState {
    state: Arc::new(InternalState {
      data_dir,
      public_dir: None,
      runtime_root_fs: None,
      site_url: config.derive(|c| Arc::new(build_site_url(c).unwrap())),
      dev: true,
      demo: false,
      auth: derive_unchecked(&config, |c| {
        Arc::new(AuthOptions::from_config(c.auth.clone()))
      }),
      jobs: derive_unchecked(&config, |_c| Arc::new(JobRegistry::new())),
      mailer: mailer.map_or_else(
        || derive_unchecked(&config, Mailer::new_from_config),
        |m| Reactive::new(m),
      ),
      record_apis: record_apis.clone(),
      config,
      json_schema_registry,
      conn: conn.clone(),
      logs_conn,
      jwt: crate::auth::jwt::test_jwt_helper(),
      subscription_manager: SubscriptionManager::new(
        conn.clone(),
        connection_metadata.clone(),
        record_apis,
      ),
      connection_metadata,
      object_store,
      wasm_runtimes: vec![],
      build_wasm_runtimes: Box::new(|| Ok(vec![])),
      test_cleanup: vec![Box::new(temp_dir)],
    }),
  });
}

// Unlike Reactive::derive, doesn't require PartialEq.
fn derive_unchecked<T, U: Clone + Send + 'static>(
  reactive: &Reactive<T>,
  f: impl Fn(&T) -> U + Send + 'static,
) -> Reactive<U>
where
  T: Clone,
{
  let derived: Reactive<U> = Reactive::new(f(&reactive.value()));

  reactive.add_observer({
    let derived = derived.clone();
    move |value| derived.update_unchecked(|_| f(value))
  });

  return derived;
}

fn build_record_api(
  conn: trailbase_sqlite::Connection,
  connection_metadata: &ConnectionMetadata,
  config: RecordApiConfig,
) -> Result<RecordApi, String> {
  let Some(ref table_name) = config.table_name else {
    return Err(format!(
      "RecordApi misses table_name configuration: {config:?}"
    ));
  };
  let table_name = QualifiedName::parse(table_name).map_err(|err| err.to_string())?;

  if let Some(table_metadata) = connection_metadata.get_table(&table_name) {
    return RecordApi::from_table(conn, table_metadata, config);
  } else if let Some(view_metadata) = connection_metadata.get_view(&table_name) {
    return RecordApi::from_view(conn, view_metadata, config);
  }

  return Err(format!("RecordApi references missing table: {config:?}"));
}

pub(crate) fn build_objectstore(
  data_dir: &DataDir,
  config: Option<&S3StorageConfig>,
) -> Result<Box<dyn ObjectStore + Send + Sync>, object_store::Error> {
  if let Some(config) = config {
    let mut builder = object_store::aws::AmazonS3Builder::from_env();

    if let Some(ref endpoint) = config.endpoint {
      builder = builder.with_endpoint(endpoint);

      if endpoint.starts_with("http://") {
        builder =
          builder.with_client_options(object_store::ClientOptions::default().with_allow_http(true))
      }
    }

    if let Some(ref region) = config.region {
      builder = builder.with_region(region);
    }

    let Some(ref bucket_name) = config.bucket_name else {
      panic!("S3StorageConfig missing 'bucket_name'.");
    };
    builder = builder.with_bucket_name(bucket_name);

    if let Some(ref access_key) = config.access_key {
      builder = builder.with_access_key_id(access_key);
    }

    if let Some(ref secret_access_key) = config.secret_access_key {
      builder = builder.with_secret_access_key(secret_access_key);
    }

    return Ok(Box::new(builder.build()?));
  }

  return Ok(Box::new(
    object_store::local::LocalFileSystem::new_with_prefix(data_dir.uploads_path())?,
  ));
}

fn build_site_url(c: &Config) -> Result<Option<url::Url>, url::ParseError> {
  if let Some(ref site_url) = c.server.site_url {
    return Ok(Some(url::Url::parse(site_url)?));
  }

  return Ok(None);
}

#[derive(Serialize)]
pub struct OAuthProvider {
  pub name: String,
  pub display_name: String,
  pub img_name: String,
}

#[derive(Serialize)]
struct AuthConfig {
  disable_password_auth: bool,
  oauth_providers: Vec<OAuthProvider>,
}

fn build_auth_config(config: &Config) -> AuthConfig {
  let oauth_providers: Vec<_> = config
    .auth
    .oauth_providers
    .iter()
    .filter_map(|(key, config)| {
      let entry = crate::auth::oauth::providers::oauth_provider_registry
        .iter()
        .find(|registered| config.provider_id == Some(registered.id as i32))?;

      let provider = (entry.factory)(key, config).ok()?;
      let name = provider.name();
      return Some(OAuthProvider {
        name: name.to_string(),
        display_name: provider.display_name().to_string(),
        img_name: crate::auth::util::oauth_provider_name_to_img(name),
      });
    })
    .collect();

  return AuthConfig {
    disable_password_auth: config.auth.disable_password_auth(),
    oauth_providers,
  };
}

const AUTH_CONFIG_KEY: &str = "config:auth";
