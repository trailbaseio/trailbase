use log::*;
use object_store::ObjectStore;
use std::path::PathBuf;
use std::sync::Arc;

use crate::auth::jwt::JwtHelper;
use crate::auth::oauth::providers::{ConfiguredOAuthProviders, OAuthProviderType};
use crate::config::proto::{Config, RecordApiConfig, S3StorageConfig};
use crate::config::{validate_config, write_config_and_vault_textproto};
use crate::constants::SITE_URL_DEFAULT;
use crate::data_dir::DataDir;
use crate::email::Mailer;
use crate::js::RuntimeHandle;
use crate::records::subscribe::SubscriptionManager;
use crate::records::RecordApi;
use crate::table_metadata::TableMetadataCache;
use crate::value_notifier::{Computed, ValueNotifier};

/// The app's internal state. AppState needs to be clonable which puts unnecessary constraints on
/// the internals. Thus rather arc once than many times.
struct InternalState {
  data_dir: DataDir,
  public_dir: Option<PathBuf>,
  dev: bool,

  oauth: Computed<ConfiguredOAuthProviders, Config>,
  mailer: Computed<Mailer, Config>,
  record_apis: Computed<Vec<(String, RecordApi)>, Config>,
  config: ValueNotifier<Config>,

  logs_conn: trailbase_sqlite::Connection,
  conn: trailbase_sqlite::Connection,

  jwt: JwtHelper,

  table_metadata: TableMetadataCache,
  subscription_manager: SubscriptionManager,
  object_store: Box<dyn ObjectStore + Send + Sync>,

  runtime: RuntimeHandle,

  #[cfg(test)]
  #[allow(unused)]
  cleanup: Vec<Box<dyn std::any::Any + Send + Sync>>,
}

pub(crate) struct AppStateArgs {
  pub data_dir: DataDir,
  pub public_dir: Option<PathBuf>,
  pub dev: bool,
  pub table_metadata: TableMetadataCache,
  pub config: Config,
  pub conn: trailbase_sqlite::Connection,
  pub logs_conn: trailbase_sqlite::Connection,
  pub jwt: JwtHelper,
  pub object_store: Box<dyn ObjectStore + Send + Sync>,
  pub js_runtime_threads: Option<usize>,
}

#[derive(Clone)]
pub struct AppState {
  state: Arc<InternalState>,
}

impl AppState {
  pub(crate) fn new(args: AppStateArgs) -> Self {
    let config = ValueNotifier::new(args.config);

    let table_metadata_clone = args.table_metadata.clone();
    let conn_clone = args.conn.clone();

    let record_apis = Computed::new(&config, move |c| {
      return c
        .record_apis
        .iter()
        .filter_map(|config| {
          match build_record_api(conn_clone.clone(), &table_metadata_clone, config.clone()) {
            Ok(api) => Some((api.api_name().to_string(), api)),
            Err(err) => {
              error!("{err}");
              None
            }
          }
        })
        .collect::<Vec<_>>();
    });

    let runtime = args
      .js_runtime_threads
      .map_or_else(RuntimeHandle::new, RuntimeHandle::new_with_threads);
    runtime.set_connection(args.conn.clone());

    AppState {
      state: Arc::new(InternalState {
        data_dir: args.data_dir,
        public_dir: args.public_dir,
        dev: args.dev,
        oauth: Computed::new(&config, |c| {
          match ConfiguredOAuthProviders::from_config(c.auth.clone()) {
            Ok(providers) => providers,
            Err(err) => {
              error!("Failed to derive configure oauth providers from config: {err}");
              ConfiguredOAuthProviders::default()
            }
          }
        }),
        mailer: build_mailer(&config, None),
        record_apis: record_apis.clone(),
        config,
        conn: args.conn.clone(),
        logs_conn: args.logs_conn,
        jwt: args.jwt,
        table_metadata: args.table_metadata.clone(),
        subscription_manager: SubscriptionManager::new(args.conn, args.table_metadata, record_apis),
        object_store: args.object_store,
        runtime,
        #[cfg(test)]
        cleanup: vec![],
      }),
    }
  }

  /// Path where TrailBase stores its data, config, migrations, and secrets.
  pub fn data_dir(&self) -> &DataDir {
    return &self.state.data_dir;
  }

  /// Optional user-prvoided public directory from where static assets are served.
  pub fn public_dir(&self) -> Option<&PathBuf> {
    return self.state.public_dir.as_ref();
  }

  pub(crate) fn dev_mode(&self) -> bool {
    return self.state.dev;
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

  pub fn version(&self) -> rustc_tools_util::VersionInfo {
    return rustc_tools_util::get_version_info!();
  }

  pub(crate) fn table_metadata(&self) -> &TableMetadataCache {
    return &self.state.table_metadata;
  }

  pub(crate) fn subscription_manager(&self) -> &SubscriptionManager {
    return &self.state.subscription_manager;
  }

  pub async fn refresh_table_cache(&self) -> Result<(), crate::table_metadata::TableLookupError> {
    self.table_metadata().invalidate_all().await
  }

  pub(crate) fn objectstore(&self) -> &(dyn ObjectStore + Send + Sync) {
    return &*self.state.object_store;
  }

  pub(crate) fn get_oauth_provider(&self, name: &str) -> Option<Arc<OAuthProviderType>> {
    return self.state.oauth.load().lookup(name).cloned();
  }

  pub(crate) fn get_oauth_providers(&self) -> Vec<(String, String)> {
    return self
      .state
      .oauth
      .load()
      .list()
      .into_iter()
      .map(|(name, display_name)| (name.to_string(), display_name.to_string()))
      .collect();
  }

  pub fn site_url(&self) -> String {
    self
      .access_config(|c| c.server.site_url.clone())
      .unwrap_or_else(|| SITE_URL_DEFAULT.to_string())
  }

  pub(crate) fn mailer(&self) -> Arc<Mailer> {
    return self.state.mailer.load().clone();
  }

  pub(crate) fn jwt(&self) -> &JwtHelper {
    return &self.state.jwt;
  }

  pub(crate) fn lookup_record_api(&self, name: &str) -> Option<RecordApi> {
    for (record_api_name, record_api) in self.state.record_apis.load().iter() {
      if record_api_name == name {
        return Some(record_api.clone());
      }
    }
    return None;
  }

  pub fn get_config(&self) -> Config {
    return (*self.state.config.load_full()).clone();
  }

  pub fn access_config<F, T>(&self, f: F) -> T
  where
    F: Fn(&Config) -> T,
  {
    return f(&self.state.config.load());
  }

  pub async fn validate_and_update_config(
    &self,
    config: Config,
    hash: Option<u64>,
  ) -> Result<(), crate::config::ConfigError> {
    validate_config(self.table_metadata(), &config)?;

    match hash {
      Some(hash) => {
        let old_config = self.state.config.load();
        if old_config.hash() == hash {
          let success = self
            .state
            .config
            .compare_and_swap(old_config, Arc::new(config));

          if !success {
            return Err(crate::config::ConfigError::Update(
              "Config compare-exchange failed".to_string(),
            ));
          }
        } else {
          return Err(crate::config::ConfigError::Update(
            "Safe config update failed: mismatching hash".to_string(),
          ));
        }
      }
      None => self.state.config.store(config.clone()),
    };

    // Write new config to the file system.
    return write_config_and_vault_textproto(
      self.data_dir(),
      self.table_metadata(),
      &self.get_config(),
    )
    .await;
  }

  #[cfg(feature = "v8")]
  pub(crate) fn script_runtime(&self) -> RuntimeHandle {
    return self.state.runtime.clone();
  }
}

fn build_mailer(
  config: &ValueNotifier<Config>,
  mailer: Option<Mailer>,
) -> Computed<Mailer, Config> {
  return Computed::new(config, move |c| {
    if let Some(mailer) = mailer.clone() {
      return mailer;
    }

    return Mailer::new_from_config(c);
  });
}

#[cfg(test)]
#[derive(Default)]
pub struct TestStateOptions {
  pub config: Option<Config>,
  pub(crate) mailer: Option<Mailer>,
}

#[cfg(test)]
pub async fn test_state(options: Option<TestStateOptions>) -> anyhow::Result<AppState> {
  use crate::auth::jwt;
  use crate::auth::oauth::providers::test::TestOAuthProvider;
  use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};
  use crate::config::validate_config;
  use crate::migrations::{apply_logs_migrations, apply_main_migrations, apply_user_migrations};

  let _ = env_logger::try_init_from_env(env_logger::Env::new().default_filter_or(
    "info,refinery_core=warn,trailbase_refinery_core=warn,tracing::span=warn,swc_ecma_codegen=off",
  ));

  let temp_dir = temp_dir::TempDir::new()?;
  tokio::fs::create_dir_all(temp_dir.child("uploads")).await?;

  let conn = {
    let mut conn = trailbase_sqlite::connect_sqlite(None, None)?;
    apply_user_migrations(&mut conn)?;
    let _new_db = apply_main_migrations(&mut conn, None)?;

    trailbase_sqlite::Connection::from_conn(conn)?
  };

  let logs_conn = {
    let mut conn = trailbase_sqlite::connect_sqlite(None, None)?;
    apply_logs_migrations(&mut conn)?;

    trailbase_sqlite::Connection::from_conn(conn)?
  };

  let table_metadata = TableMetadataCache::new(conn.clone()).await?;

  let build_default_config = || {
    // Construct a fabricated config for tests and make sure it's valid.
    let mut config = Config::new_with_custom_defaults();

    config.email.smtp_host = Some("host".to_string());
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
        provider_id: Some(OAuthProviderId::Custom as i32),
        ..Default::default()
      },
    );

    // NOTE: The below "append" semantics are different from prod's override behavior, to avoid
    // races between concurrent tests. The registry needs to be global for the sqlite extensions
    // to access (unless we find a better way to bind the two).
    for schema in &config.schemas {
      trailbase_sqlite::schema::set_user_schema(
        schema.name.as_ref().unwrap(),
        Some(serde_json::to_value(schema.schema.as_ref().unwrap()).unwrap()),
      )
      .unwrap();
    }

    config
  };

  let config = options
    .as_ref()
    .and_then(|o| o.config.clone())
    .unwrap_or_else(build_default_config);
  validate_config(&table_metadata, &config).unwrap();
  let config = ValueNotifier::new(config);

  let main_conn_clone = conn.clone();
  let table_metadata_clone = table_metadata.clone();

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
  } else {
    build_objectstore(&data_dir, None).unwrap()
  };

  let record_apis = Computed::new(&config, move |c| {
    return c
      .record_apis
      .iter()
      .filter_map(|config| {
        let api = build_record_api(
          main_conn_clone.clone(),
          &table_metadata_clone,
          config.clone(),
        )
        .unwrap();

        return Some((api.api_name().to_string(), api));
      })
      .collect::<Vec<_>>();
  });

  let runtime = RuntimeHandle::new();
  runtime.set_connection(conn.clone());

  return Ok(AppState {
    state: Arc::new(InternalState {
      data_dir,
      public_dir: None,
      dev: true,
      oauth: Computed::new(&config, |c| {
        ConfiguredOAuthProviders::from_config(c.auth.clone()).unwrap()
      }),
      mailer: build_mailer(&config, options.and_then(|o| o.mailer)),
      record_apis: record_apis.clone(),
      config,
      conn: conn.clone(),
      logs_conn,
      jwt: jwt::test_jwt_helper(),
      table_metadata: table_metadata.clone(),
      subscription_manager: SubscriptionManager::new(conn, table_metadata, record_apis),
      object_store,
      runtime,
      cleanup: vec![Box::new(temp_dir)],
    }),
  });
}

fn build_record_api(
  conn: trailbase_sqlite::Connection,
  table_metadata_cache: &TableMetadataCache,
  config: RecordApiConfig,
) -> Result<RecordApi, String> {
  let Some(ref table_name) = config.table_name else {
    return Err(format!(
      "RecordApi misses table_name configuration: {config:?}"
    ));
  };

  if let Some(table_metadata) = table_metadata_cache.get(table_name) {
    return RecordApi::from_table(conn, (*table_metadata).clone(), config);
  } else if let Some(view) = table_metadata_cache.get_view(table_name) {
    return RecordApi::from_view(conn, (*view).clone(), config);
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
