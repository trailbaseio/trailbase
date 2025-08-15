use log::*;
use object_store::ObjectStore;
use reactivate::{Merge, Reactive};
use std::path::PathBuf;
use std::sync::Arc;
use trailbase_schema::QualifiedName;
use trailbase_wasm::Runtime;

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

/// The app's internal state. AppState needs to be clonable which puts unnecessary constraints on
/// the internals. Thus rather arc once than many times.
struct InternalState {
  data_dir: DataDir,
  public_dir: Option<PathBuf>,
  site_url: Reactive<Arc<Option<url::Url>>>,
  dev: bool,
  demo: bool,

  auth: Reactive<Arc<AuthOptions>>,
  jobs: Reactive<Arc<JobRegistry>>,
  mailer: Reactive<Mailer>,
  record_apis: Reactive<Arc<Vec<(String, RecordApi)>>>,
  config: Reactive<Config>,

  conn: trailbase_sqlite::Connection,
  logs_conn: trailbase_sqlite::Connection,

  jwt: JwtHelper,

  schema_metadata: Reactive<Arc<SchemaMetadataCache>>,
  subscription_manager: SubscriptionManager,
  object_store: Arc<dyn ObjectStore + Send + Sync>,

  runtime: RuntimeHandle,

  wasm_runtime: Option<Arc<Runtime>>,

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

    let schema_metadata = Reactive::new(Arc::new(args.schema_metadata));
    let record_apis = {
      let conn = args.conn.clone();
      let m = (&config, &schema_metadata).merge();

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

    let runtime = build_js_runtime(args.conn.clone(), args.js_runtime_threads);
    let wasm_runtime = crate::wasm::build_wasm_runtime(&args.data_dir, args.conn.clone())
      .ok()
      .flatten()
      .map(Arc::new);

    AppState {
      state: Arc::new(InternalState {
        data_dir: args.data_dir,
        public_dir: args.public_dir,
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
        conn: args.conn.clone(),
        logs_conn: args.logs_conn,
        jwt: args.jwt,
        subscription_manager: SubscriptionManager::new(
          args.conn,
          schema_metadata.clone(),
          record_apis,
        ),
        schema_metadata,
        object_store,
        runtime,
        wasm_runtime,
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

  pub fn version(&self) -> trailbase_build::version::VersionInfo {
    return trailbase_build::get_version_info!();
  }

  pub(crate) fn schema_metadata(&self) -> Arc<SchemaMetadataCache> {
    return self.state.schema_metadata.value();
  }

  pub(crate) fn subscription_manager(&self) -> &SubscriptionManager {
    return &self.state.subscription_manager;
  }

  pub async fn rebuild_schema_cache(
    &self,
  ) -> Result<(), crate::schema_metadata::SchemaLookupError> {
    let metadata = SchemaMetadataCache::new(&self.state.conn).await?;
    self.state.schema_metadata.set(Arc::new(metadata));
    return Ok(());
    // self.schema_metadata().invalidate_all().await
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
  ) -> Result<(), crate::config::ConfigError> {
    validate_config(&self.schema_metadata(), &config)?;

    match hash {
      Some(hash) => {
        let mut error: Option<crate::config::ConfigError> = None;
        let err = &mut error;
        self.state.config.update(move |old| {
          if hash_config(old) != hash {
            let _ = err.insert(crate::config::ConfigError::Update(
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
      None => self.state.config.set(config),
    };

    // Write new config to the file system.
    return write_config_and_vault_textproto(
      self.data_dir(),
      &self.schema_metadata(),
      &self.get_config(),
    )
    .await;
  }

  #[cfg(feature = "v8")]
  pub(crate) fn script_runtime(&self) -> RuntimeHandle {
    return self.state.runtime.clone();
  }

  pub(crate) fn wasm_runtime(&self) -> Option<Arc<Runtime>> {
    return self.state.wasm_runtime.clone();
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
  use reactivate::Merge;

  use crate::auth::jwt;
  use crate::auth::oauth::providers::test::TestOAuthProvider;
  use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};
  use crate::config::validate_config;

  let _ = env_logger::try_init_from_env(
    env_logger::Env::new().default_filter_or("info,trailbase_refinery=warn,log::span=warn"),
  );

  let temp_dir = temp_dir::TempDir::new()?;
  tokio::fs::create_dir_all(temp_dir.child("uploads")).await?;

  let (conn, new) = crate::connection::init_main_db(None, None)?;
  assert!(new);
  let logs_conn = crate::connection::init_logs_db(None)?;

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

  let schema_metadata = Arc::new(SchemaMetadataCache::new(&conn).await?);
  let config = options
    .as_ref()
    .and_then(|o| o.config.clone())
    .unwrap_or_else(build_default_config);
  validate_config(&schema_metadata, &config).unwrap();
  let config = Reactive::new(config);

  let main_conn_clone = conn.clone();

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

  let schema_metadata = Reactive::new(schema_metadata);
  let record_apis = {
    // let schema_metadata = schema_metadata.clone();
    let m = (&config, &schema_metadata).merge();
    derive_unchecked(&m, move |(c, metadata)| {
      return Arc::new(
        c.record_apis
          .iter()
          .filter_map(|config| {
            let api = build_record_api(main_conn_clone.clone(), &metadata, config.clone()).unwrap();

            return Some((api.api_name().to_string(), api));
          })
          .collect::<Vec<_>>(),
      );
    })
  };

  return Ok(AppState {
    state: Arc::new(InternalState {
      data_dir,
      public_dir: None,
      site_url: config.derive(|c| Arc::new(build_site_url(c).unwrap())),
      dev: true,
      demo: false,
      auth: derive_unchecked(&config, |c| {
        Arc::new(AuthOptions::from_config(c.auth.clone()))
      }),
      jobs: derive_unchecked(&config, |_c| Arc::new(JobRegistry::new())),
      mailer: if let Some(mailer) = options.and_then(|o| o.mailer) {
        Reactive::new(mailer)
      } else {
        derive_unchecked(&config, Mailer::new_from_config)
      },
      record_apis: record_apis.clone(),
      config,
      conn: conn.clone(),
      logs_conn,
      jwt: jwt::test_jwt_helper(),
      subscription_manager: SubscriptionManager::new(
        conn.clone(),
        schema_metadata.clone(),
        record_apis,
      ),
      schema_metadata,
      object_store,
      runtime: build_js_runtime(conn, None),
      wasm_runtime: None,
      cleanup: vec![Box::new(temp_dir)],
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

fn build_js_runtime(conn: trailbase_sqlite::Connection, threads: Option<usize>) -> RuntimeHandle {
  let runtime = if let Some(threads) = threads {
    RuntimeHandle::singleton_or_init_with_threads(threads)
  } else {
    RuntimeHandle::singleton()
  };

  if cfg!(test) {
    lazy_static::lazy_static! {
      static ref START: std::sync::Once = std::sync::Once::new();
    }
    START.call_once(|| {
      register_database_functions(&runtime, conn);
    });
  } else {
    register_database_functions(&runtime, conn);
  }

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
