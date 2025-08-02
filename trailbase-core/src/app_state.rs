use log::*;
use object_store::ObjectStore;
use std::path::PathBuf;
use std::sync::Arc;
use trailbase_schema::QualifiedName;

use crate::auth::jwt::JwtHelper;
use crate::auth::options::AuthOptions;
use crate::config::proto::{Config, RecordApiConfig, S3StorageConfig, hash_config};
use crate::config::{validate_config, write_config_and_vault_textproto};
use crate::data_dir::DataDir;
use crate::email::Mailer;
use crate::js::{RuntimeHandle, register_database_functions};
use crate::records::RecordApi;
use crate::records::subscribe::SubscriptionManager;
use crate::scheduler::{JobRegistry, build_job_registry_from_config};
use crate::schema_metadata::SchemaMetadataCache;
use crate::value_notifier::{Computed, Guard, ValueNotifier};

/// The app's internal state. AppState needs to be clonable which puts unnecessary constraints on
/// the internals. Thus rather arc once than many times.
struct InternalState {
  data_dir: DataDir,
  public_dir: Option<PathBuf>,
  site_url: Computed<Option<url::Url>>,
  dev: bool,
  demo: bool,

  auth: Computed<AuthOptions>,
  jobs: Computed<JobRegistry>,
  mailer: Computed<Mailer>,
  record_apis: Computed<Vec<(String, RecordApi)>>,
  config: ValueNotifier<Config>,

  conn: trailbase_sqlite::Connection,
  logs_conn: trailbase_sqlite::Connection,

  jwt: JwtHelper,

  schema_metadata: SchemaMetadataCache,
  subscription_manager: SubscriptionManager,
  object_store: Arc<dyn ObjectStore + Send + Sync>,

  runtime: RuntimeHandle,

  #[cfg(test)]
  #[allow(unused)]
  cleanup: Vec<Box<dyn std::any::Any + Send + Sync>>,
}

pub(crate) struct AppStateArgs {
  pub data_dir: DataDir,
  pub public_url: Option<url::Url>,
  pub public_dir: Option<PathBuf>,
  pub dev: bool,
  pub demo: bool,
  pub schema_metadata: SchemaMetadataCache,
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

    let public_url = args.public_url.clone();
    let site_url = Computed::new(&config, move |site_url| -> Option<url::Url> {
      if let Some(ref public_url) = public_url {
        log::info!("Public url provided: {public_url:?}");
        return Some(public_url.clone());
      }

      return build_site_url(site_url)
        .map_err(|err| {
          error!("Failed to parse `site_url`: {err}");
          return err;
        })
        .ok()
        .flatten();
    });

    let record_apis = {
      let schema_metadata_clone = args.schema_metadata.clone();
      let conn_clone = args.conn.clone();

      Computed::new(&config, move |c| {
        return c
          .record_apis
          .iter()
          .filter_map(|config| {
            match build_record_api(conn_clone.clone(), &schema_metadata_clone, config.clone()) {
              Ok(api) => Some((api.api_name().to_string(), api)),
              Err(err) => {
                error!("{err}");
                None
              }
            }
          })
          .collect::<Vec<_>>();
      })
    };

    let object_store: Arc<dyn ObjectStore + Send + Sync> = args.object_store.into();
    let jobs_input = (
      args.data_dir.clone(),
      args.conn.clone(),
      args.logs_conn.clone(),
      object_store.clone(),
    );

    let runtime = build_js_runtime(args.conn.clone(), args.js_runtime_threads);

    AppState {
      state: Arc::new(InternalState {
        data_dir: args.data_dir,
        public_dir: args.public_dir,
        site_url,
        dev: args.dev,
        demo: args.demo,
        auth: Computed::new(&config, |c| AuthOptions::from_config(c.auth.clone())),
        jobs: Computed::new(&config, move |c| {
          debug!("building jobs from config");

          let (data_dir, conn, logs_conn, object_store) = &jobs_input;

          return build_job_registry_from_config(
            c,
            data_dir,
            conn,
            logs_conn,
            object_store.clone(),
          )
          .unwrap_or_else(|err| {
            error!("Failed to build JobRegistry for cron jobs: {err}");
            return JobRegistry::new();
          });
        }),
        mailer: Computed::new(&config, Mailer::new_from_config),
        record_apis: record_apis.clone(),
        config,
        conn: args.conn.clone(),
        logs_conn: args.logs_conn,
        jwt: args.jwt,
        schema_metadata: args.schema_metadata.clone(),
        subscription_manager: SubscriptionManager::new(
          args.conn,
          args.schema_metadata,
          record_apis,
        ),
        object_store,
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

  pub(crate) fn demo_mode(&self) -> bool {
    return self.state.demo;
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

  pub fn version(&self) -> trailbase_assets::version::VersionInfo {
    return trailbase_assets::get_version_info!();
  }

  pub(crate) fn schema_metadata(&self) -> &SchemaMetadataCache {
    return &self.state.schema_metadata;
  }

  pub(crate) fn subscription_manager(&self) -> &SubscriptionManager {
    return &self.state.subscription_manager;
  }

  pub async fn refresh_table_cache(&self) -> Result<(), crate::schema_metadata::SchemaLookupError> {
    self.schema_metadata().invalidate_all().await
  }

  pub(crate) fn touch_config(&self) {
    self.state.config.touch();
  }

  pub(crate) fn objectstore(&self) -> &(dyn ObjectStore + Send + Sync) {
    return &*self.state.object_store;
  }

  pub(crate) fn jobs(&self) -> Guard<Arc<JobRegistry>> {
    return self.state.jobs.load();
  }

  pub(crate) fn auth_options(&self) -> Guard<Arc<AuthOptions>> {
    return self.state.auth.load();
  }

  pub fn site_url(&self) -> Arc<Option<url::Url>> {
    return self.state.site_url.load_full();
  }

  pub(crate) fn mailer(&self) -> Guard<Arc<Mailer>> {
    return self.state.mailer.load();
  }

  pub(crate) fn jwt(&self) -> &JwtHelper {
    return &self.state.jwt;
  }

  pub fn lookup_record_api(&self, name: &str) -> Option<RecordApi> {
    for (record_api_name, record_api) in self.state.record_apis.load().iter() {
      if record_api_name == name {
        return Some(record_api.clone());
      }
    }
    return None;
  }

  pub fn get_config(&self) -> Config {
    return (**self.state.config.load()).clone();
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
    hash: Option<String>,
  ) -> Result<(), crate::config::ConfigError> {
    validate_config(self.schema_metadata(), &config)?;

    match hash {
      Some(hash) => {
        let old_config = self.state.config.load();
        if hash_config(&old_config) != hash {
          return Err(crate::config::ConfigError::Update(
            "Config update failed: mismatching or stale hash".to_string(),
          ));
        }

        let success = self
          .state
          .config
          .compare_and_swap(old_config, Arc::new(config));

        if !success {
          return Err(crate::config::ConfigError::Update(
            "Config compare-exchange failed".to_string(),
          ));
        }
      }
      None => self.state.config.store(config.clone()),
    };

    // Write new config to the file system.
    return write_config_and_vault_textproto(
      self.data_dir(),
      self.schema_metadata(),
      &self.get_config(),
    )
    .await;
  }

  #[cfg(feature = "v8")]
  pub(crate) fn script_runtime(&self) -> RuntimeHandle {
    return self.state.runtime.clone();
  }
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

  let _ = env_logger::try_init_from_env(
    env_logger::Env::new()
      .default_filter_or("info,trailbase_refinery=warn,log::span=warn,swc_ecma_codegen=off"),
  );

  let temp_dir = temp_dir::TempDir::new()?;
  tokio::fs::create_dir_all(temp_dir.child("uploads")).await?;

  let (conn, new) = crate::connection::init_main_db(None, None, None)?;
  assert!(new);
  let logs_conn = crate::connection::init_logs_db(None)?;

  let schema_metadata = SchemaMetadataCache::new(conn.clone()).await?;

  let build_default_config = || {
    // Construct a fabricated config for tests and make sure it's valid.
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

    // NOTE: The below "append" semantics are different from prod's override behavior, to avoid
    // races between concurrent tests. The registry needs to be global for the sqlite extensions
    // to access (unless we find a better way to bind the two).
    for schema in &config.schemas {
      trailbase_schema::registry::set_user_schema(
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
  validate_config(&schema_metadata, &config).unwrap();
  let config = ValueNotifier::new(config);

  let main_conn_clone = conn.clone();
  let schema_metadata_clone = schema_metadata.clone();

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

  let record_apis = Computed::new(&config, move |c| {
    return c
      .record_apis
      .iter()
      .filter_map(|config| {
        let api = build_record_api(
          main_conn_clone.clone(),
          &schema_metadata_clone,
          config.clone(),
        )
        .unwrap();

        return Some((api.api_name().to_string(), api));
      })
      .collect::<Vec<_>>();
  });

  fn build_mailer(c: &ValueNotifier<Config>, mailer: Option<Mailer>) -> Computed<Mailer> {
    return Computed::new(c, move |c| {
      return mailer.clone().unwrap_or_else(|| Mailer::new_from_config(c));
    });
  }

  return Ok(AppState {
    state: Arc::new(InternalState {
      data_dir,
      public_dir: None,
      site_url: Computed::new(&config, |c| build_site_url(c).unwrap()),
      dev: true,
      demo: false,
      auth: Computed::new(&config, |c| AuthOptions::from_config(c.auth.clone())),
      jobs: Computed::new(&config, |_c| JobRegistry::new()),
      mailer: build_mailer(&config, options.and_then(|o| o.mailer)),
      record_apis: record_apis.clone(),
      config,
      conn: conn.clone(),
      logs_conn,
      jwt: jwt::test_jwt_helper(),
      schema_metadata: schema_metadata.clone(),
      subscription_manager: SubscriptionManager::new(conn.clone(), schema_metadata, record_apis),
      object_store,
      runtime: build_js_runtime(conn, None),
      cleanup: vec![Box::new(temp_dir)],
    }),
  });
}

#[cfg(test)]
static START: std::sync::Once = std::sync::Once::new();

fn build_js_runtime(conn: trailbase_sqlite::Connection, threads: Option<usize>) -> RuntimeHandle {
  let runtime = if let Some(threads) = threads {
    RuntimeHandle::singleton_or_init_with_threads(threads)
  } else {
    RuntimeHandle::singleton()
  };

  #[cfg(test)]
  START.call_once(|| {
    register_database_functions(&runtime, conn);
  });

  #[cfg(not(test))]
  register_database_functions(&runtime, conn);

  return runtime;
}

fn build_record_api(
  conn: trailbase_sqlite::Connection,
  schema_metadata_cache: &SchemaMetadataCache,
  config: RecordApiConfig,
) -> Result<RecordApi, String> {
  let Some(ref table_name) = config.table_name else {
    return Err(format!(
      "RecordApi misses table_name configuration: {config:?}"
    ));
  };
  let table_name = QualifiedName::parse(table_name).map_err(|err| err.to_string())?;

  if let Some(schema_metadata) = schema_metadata_cache.get_table(&table_name) {
    return RecordApi::from_table(conn, &schema_metadata, config);
  } else if let Some(view) = schema_metadata_cache.get_view(&table_name) {
    return RecordApi::from_view(conn, &view, config);
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
