use log::*;
use object_store::ObjectStore;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use trailbase_extension::jsonschema::JsonSchemaRegistry;
use trailbase_reactive::{AsyncReactive, DeriveInput, Reactive};

use crate::auth::jwt::JwtHelper;
use crate::auth::options::AuthOptions;
use crate::config::proto::{
  Config, JsonSchemaConfig, RecordApiConfig, S3StorageConfig, hash_config,
};
use crate::config::{
  ConfigError, load_or_init_config_textproto, validate_config, write_config_and_vault_textproto,
};
use crate::connection::{BuildOptions, ConnectionEntry, ConnectionError, ConnectionManager};
use crate::constants::USER_TABLE;
use crate::data_dir::DataDir;
use crate::email::Mailer;
use crate::init_error::InitError;
use crate::metadata::load_or_init_metadata_textproto;
use crate::rand::random_alphanumeric;
use crate::records::RecordApi;
use crate::records::subscribe::manager::SubscriptionManager;
use crate::scheduler::{JobRegistry, build_job_registry_from_config};

#[derive(Default)]
pub struct InitArgs {
  pub data_dir: DataDir,
  pub public_url: Option<url::Url>,
  pub runtime_root_fs: Option<PathBuf>,
  pub geoip_db_path: Option<PathBuf>,

  pub dev: bool,
  pub demo: bool,
  pub wasm_tokio_runtime: Option<tokio::runtime::Handle>,

  pub pg_uri: Option<String>,
}

/// The app's internal state. AppState needs to be clonable which puts unnecessary constraints on
/// the internals. Thus rather arc once than many times.
struct InternalState {
  data_dir: DataDir,
  start_time: std::time::SystemTime,

  site_url: Reactive<Arc<Option<url::Url>>>,
  dev: bool,
  demo: bool,

  auth: Reactive<Arc<AuthOptions>>,
  jobs: Reactive<Arc<JobRegistry>>,
  mailer: Reactive<Mailer>,
  config: Reactive<Config>,
  json_schema_registry: Arc<parking_lot::RwLock<JsonSchemaRegistry>>,

  // TODO: Maybe remove main `conn` in favor of connection manager. Note that this is currently
  // also used for the state.user_conn().
  conn: trailbase_sqlite::Connection,
  session_conn: trailbase_sqlite::Connection,
  logs_conn: trailbase_sqlite::Connection,
  connection_manager: ConnectionManager,

  jwt: JwtHelper,

  record_apis: AsyncReactive<HashMap<String, RecordApi>>,
  subscription_manager: SubscriptionManager,
  object_store: Arc<dyn ObjectStore>,

  /// Actual WASM runtimes.
  #[cfg(feature = "wasm")]
  wasm: crate::wasm::WasmState,

  #[cfg(test)]
  #[allow(unused)]
  pg_uri: Option<String>,

  #[cfg(test)]
  #[allow(unused)]
  test_cleanup: Vec<Box<dyn std::any::Any + Send + Sync>>,
}

#[derive(Clone)]
pub struct AppState {
  state: Arc<InternalState>,
}

impl AppState {
  pub async fn init(args: InitArgs) -> Result<(bool, Self), InitError> {
    validate_path(args.runtime_root_fs.as_ref())?;
    validate_path(args.geoip_db_path.as_ref())?;

    // First create directory structure.
    args.data_dir.ensure_directory_structure().await?;

    // Then open or init new databases.
    let logs_conn = crate::connection::init_logs_db(Some(&args.data_dir))?;
    let session_conn = crate::connection::init_session_db(Some(&args.data_dir))?;

    let json_schema_registry = Arc::new(parking_lot::RwLock::new(
      trailbase_schema::registry::build_json_schema_registry(vec![])?,
    ));

    if let Some(config) = crate::config::maybe_load_config_textproto_unverified(&args.data_dir)? {
      update_json_schema_registry(&config.schemas, &json_schema_registry)?;
    }

    let (connection_manager, new_db) = ConnectionManager::new(crate::connection::Options {
      data_dir: args.data_dir.clone(),
      json_schema_registry: json_schema_registry.clone(),
      sqlite_function_runtimes: cfg_select! {
        feature = "wasm" => {
          crate::wasm::build_sync_wasm_runtimes_for_components(
            args.data_dir.root().join("wasm"),
            args.runtime_root_fs.as_deref(),
            args.dev,
          )
          .await
          .map_err(|err| InitError::ScriptError(err.to_string()))?
        }
        _ => vec![],
      },
      // TODO: Wire up from config, if/when PG is supported.
      pg_uri: cfg_select! {
        feature = "pg" => args.pg_uri,
        _ => None,
      },
    })
    .await?;

    // Read config or write default one. Ensures config is validated.
    let config = load_or_init_config_textproto(&args.data_dir, &connection_manager).await?;

    // Load the `<depot>/metadata.textproto`.
    let _metadata = load_or_init_metadata_textproto(&args.data_dir).await?;

    let jwt = JwtHelper::init_from_path(&args.data_dir).await?;

    // Init geoip if present.
    let geoip_db_path = args
      .geoip_db_path
      .unwrap_or_else(|| args.data_dir.root().join("GeoLite2-Country.mmdb"));
    if let Err(err) = trailbase_extension::geoip::load_geoip_db(geoip_db_path.clone()) {
      debug!("Failed to load maxmind geoip DB '{geoip_db_path:?}': {err}");
    }

    let object_store = Arc::new(build_objectstore(
      &args.data_dir,
      config.server.s3_storage_config.as_ref(),
    )?);

    let app_state = {
      // Build reactive stuff.
      let config = Reactive::new(config);

      let site_url = config.derive({
        let public_url = args.public_url.clone();
        move |config| {
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
        }
      });

      let record_apis = build_record_apis(
        connection_manager.clone(),
        config.derive(|c| c.record_apis.clone()),
      )
      .await;

      let main_conn = connection_manager.main_entry().connection;

      AppState {
        state: Arc::new(InternalState {
          data_dir: args.data_dir.clone(),
          start_time: std::time::SystemTime::now(),
          site_url,
          dev: args.dev,
          demo: args.demo,
          auth: config.derive_unchecked(|c| {
            Arc::new(AuthOptions::from_config(
              c.server.site_url.as_deref(),
              c.auth.clone(),
            ))
          }),
          jobs: config.derive_unchecked({
            let jobs_input = (
              args.data_dir.clone(),
              connection_manager.clone(),
              logs_conn.clone(),
              session_conn.clone(),
              object_store.clone(),
            );
            move |c| {
              debug!("(re-)building jobs from config");

              let (data_dir, conn_mgr, logs_conn, session_conn, object_store) = &jobs_input;

              return Arc::new(
                build_job_registry_from_config(
                  c,
                  data_dir,
                  conn_mgr,
                  logs_conn,
                  session_conn,
                  object_store.clone(),
                )
                .unwrap_or_else(|err| {
                  error!("Failed to build JobRegistry for cron jobs: {err}");
                  return JobRegistry::new();
                }),
              );
            }
          }),
          mailer: config.derive_unchecked(Mailer::new_from_config),
          config: config.clone(),
          json_schema_registry,
          conn: (*main_conn).clone(),
          session_conn,
          logs_conn,
          connection_manager,
          jwt,
          record_apis: record_apis.clone(),
          subscription_manager: SubscriptionManager::new(record_apis),
          object_store,
          #[cfg(feature = "wasm")]
          wasm: crate::wasm::WasmState::init(
            &args.data_dir,
            config,
            (*main_conn).clone(),
            args.wasm_tokio_runtime,
            args.runtime_root_fs,
            args.dev,
          ),
          #[cfg(test)]
          pg_uri: None,
          #[cfg(test)]
          test_cleanup: vec![],
        }),
      }
    };

    if new_db {
      let num_admins: i64 = app_state
        .user_conn()
        .read_query_row_get(
          format!("SELECT COUNT(*) FROM {USER_TABLE} WHERE admin = TRUE"),
          (),
          0,
        )
        .await?
        .unwrap_or(0);

      if num_admins == 0 {
        let email = "admin@localhost";
        let username = "admin";
        let password = random_alphanumeric(20);
        let hashed_password = crate::auth::password::hash_password(&password)?;

        app_state
          .user_conn()
          .execute(
            format!(
              r#"
              INSERT INTO {USER_TABLE}
                (email, username, password_hash, admin)
              VALUES
                ($1, $2, $3, TRUE)
            "#
            ),
            trailbase_sqlite::params!(email.to_string(), username.to_string(), hashed_password),
          )
          .await?;

        info!("Created new admin user:\n\temail: '{email}'\n\tpassword: '{password}'");
      }
    }

    if cfg!(debug_assertions) {
      let text_config = app_state.get_config().to_text()?;
      debug!("Config: {text_config}");
    }

    return Ok((new_db, app_state));
  }

  /// Path where TrailBase stores its data, config, migrations, and secrets.
  pub fn data_dir(&self) -> &DataDir {
    return &self.state.data_dir;
  }

  pub fn start_time(&self) -> std::time::SystemTime {
    return self.state.start_time;
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

  #[cfg(test)]
  pub fn conn(&self) -> &trailbase_sqlite::Connection {
    return &self.state.conn;
  }

  pub fn user_conn(&self) -> &trailbase_sqlite::Connection {
    return &self.state.conn;
  }

  pub fn session_conn(&self) -> &trailbase_sqlite::Connection {
    return &self.state.session_conn;
  }

  pub fn logs_conn(&self) -> &trailbase_sqlite::Connection {
    return &self.state.logs_conn;
  }

  pub fn connection_manager(&self) -> ConnectionManager {
    return self.state.connection_manager.clone();
  }

  pub fn version(&self) -> trailbase_build::version::VersionInfo {
    return trailbase_build::get_version_info!();
  }

  pub(crate) fn subscription_manager(&self) -> &SubscriptionManager {
    return &self.state.subscription_manager;
  }

  pub async fn rebuild_connection_metadata(
    &self,
  ) -> Result<(), crate::connection::ConnectionError> {
    self.state.connection_manager.rebuild_metadata().await?;

    // We typically rebuild the schema representations when the DB schemas change, which in turn
    // can invalidate the config, e.g. an API may reference a deleted table. Let's make sure to
    // check. Note however that this is tricky to deal with, since the schema changes have already
    // happened rendering the current config invalid. Unlike a config update, it's too late to
    // reject anything.
    let config = self.get_config();
    validate_config(&self.state.connection_manager, &config)
      .await
      .map_err(|err| {
        log::error!("Schema change invalidated config: {err}");
        return err;
      })?;

    // Rebuild RecordApi including schemas. This is necessary e.g. after schema changes.
    let connection_manager = self.state.connection_manager.clone();
    let record_api_config = Arc::new(config.record_apis.clone());
    self
      .state
      .record_apis
      .update_unchecked(async |prev| {
        let next = build_record_apis_impl(connection_manager, Some(prev), record_api_config).await;

        return next;
      })
      .await;

    return Ok(());
  }

  pub(crate) fn objectstore(&self) -> &Arc<dyn ObjectStore> {
    return &self.state.object_store;
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
    return self.state.record_apis.snapshot().get(name).cloned();
  }

  pub fn get_config(&self) -> Arc<Config> {
    return self.state.config.ptr();
  }

  pub fn access_config<F, T>(&self, f: F) -> T
  where
    F: FnOnce(&Config) -> T,
  {
    return f(&self.state.config.ptr());
  }

  pub async fn validate_and_update_config(
    &self,
    config: Config,
    hash: Option<String>,
  ) -> Result<(), ConfigError> {
    let connection_manager = self.connection_manager();
    validate_config(&connection_manager, &config).await?;

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
        self.state.config.update(|_old| config);
      }
    };

    let new_config = self.get_config();

    {
      if update_json_schema_registry(&new_config.schemas, &self.state.json_schema_registry)
        .unwrap_or(true)
        && let Err(err) = self.rebuild_connection_metadata().await
      {
        log::warn!("reloading JSON schema cache failed: {err}");
      }
    }

    // Write new config to the file system.
    write_config_and_vault_textproto(self.data_dir(), &connection_manager, &new_config).await?;

    // After updating the config we need to poll record apis to make sure they're up-to-date.
    let _wait_for_snapshot_update = self.state.record_apis.ptr().await;

    return Ok(());
  }

  #[cfg(feature = "wasm")]
  pub(crate) fn wasm(&self) -> &crate::wasm::WasmState {
    return &self.state.wasm;
  }
}

/// Returns true if schemas were registered.
pub(crate) fn update_json_schema_registry(
  config: &[JsonSchemaConfig],
  registry: &parking_lot::RwLock<JsonSchemaRegistry>,
) -> Result<bool, ConfigError> {
  if !config.is_empty() {
    let schemas: Vec<_> = config
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

async fn build_record_apis(
  connection_manager: ConnectionManager,
  record_api_configs: Reactive<Vec<RecordApiConfig>>,
) -> AsyncReactive<HashMap<String, RecordApi>> {
  return record_api_configs
    .derive_unchecked_async(move |DeriveInput { prev, dep: configs }| {
      return build_record_apis_impl(connection_manager.clone(), prev.cloned(), configs.clone());
    })
    .await;
}

async fn build_record_apis_impl(
  connection_manager: ConnectionManager,
  prev: Option<Arc<HashMap<String, RecordApi>>>,
  record_api_configs: Arc<Vec<RecordApiConfig>>,
) -> HashMap<String, RecordApi> {
  // Re-use existing connection when possible to keep subscriptions alive.
  //
  // WARN: We need to be very careful to how we rebuild RecordAPIs, since long-lived
  // subscriptions may be tied to specific connections. So we need to keep connection alive
  // whenever possible, e.g. an ACL changing for one API isn't a good reason to drop
  // subscriptions on all APIs.
  let get_conn =
    async move |api_name: &str, attached_databases: &[String]| -> Result<_, ConnectionError> {
      let ConnectionEntry {
        connection: conn,
        metadata,
      } = if attached_databases.is_empty() {
        connection_manager.main_entry()
      } else {
        connection_manager
          .get_entry(BuildOptions {
            is_main: true,
            attached_databases: Some(attached_databases.iter().cloned().collect()),
            ..Default::default()
          })
          .await?
      };

      if let Some((_, candidate)) =
        prev
          .as_ref()
          .and_then(|prev: &Arc<HashMap<String, RecordApi>>| {
            return prev.iter().find(|(_name, api)| api.api_name() == api_name);
          })
        && candidate.attached_databases() == attached_databases
      {
        // NOTE: We must use latest metadata to work recorrectly on schema changes.
        return Ok((candidate.conn().clone(), metadata));
      };

      return Ok((conn, metadata));
    };

  let mut next: HashMap<String, RecordApi> = HashMap::new();
  for config in record_api_configs.iter() {
    let (conn, metadata) = match get_conn(config.name(), &config.attached_databases).await {
      Ok(x) => x,
      Err(err) => {
        log::error!("Failed to get conn for record API {}: {err}", config.name());
        continue;
      }
    };

    match RecordApi::build(conn, metadata, config.clone()) {
      Ok(api) => {
        next.insert(api.api_name().to_string(), api);
      }
      Err(err) => {
        log::error!("Failed to build record API {}: {err}", config.name());
      }
    };
  }

  return next;
}

pub(crate) fn build_objectstore(
  data_dir: &DataDir,
  config: Option<&S3StorageConfig>,
) -> Result<Box<dyn ObjectStore>, object_store::Error> {
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

#[cfg(test)]
mod test_utils {
  use super::*;

  /// Construct a fabricated config for tests and make sure it's valid.
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

  #[derive(Default)]
  pub struct TestStateOptions {
    pub config: Option<Config>,
    pub json_schema_registry: Option<JsonSchemaRegistry>,
    pub(crate) mailer: Option<Mailer>,
  }

  pub async fn test_state(options: Option<TestStateOptions>) -> anyhow::Result<AppState> {
    let _ = env_logger::try_init_from_env(
      env_logger::Env::new().default_filter_or("info,trailbase_refinery=warn,log::span=warn"),
    );

    let temp_dir = temp_dir::TempDir::new()?;
    let data_dir = DataDir(temp_dir.path().to_path_buf());
    data_dir.ensure_directory_structure().await.unwrap();

    let (pg_db, pg_uri) = if cfg!(feature = "pg-test") {
      let extensions = [
        // Enable case-insensitive text columns.
        pglite_oxide::extensions::CITEXT,
        // Enable UUIDv7 support.
        pglite_oxide::extensions::PG_UUIDV7,
        // NOTE: pgcrypto and postgis, which would be interesting for us, are not currently
        // supported: https://github.com/f0rr0/pglite-oxide/blob/main/docs/EXTENSIONS.md
      ];

      // Start PgLite.
      let sock = data_dir.main_db_path().join(".s.PGSQL.5432");

      let db = Arc::new(parking_lot::Mutex::new(Some(
        pglite_oxide::PgliteServer::builder()
          .fresh_temporary()
          .extensions(extensions)
          .unix(&sock)
          .start()?,
      )));

      let handle = tokio::runtime::Handle::current();
      let runtime_monitor = tokio_metrics::RuntimeMonitor::new(&handle);

      // NOTE: During CI, we have random tests occasionally time out. This is an attempt
      // to get ahead of it.
      let _ = std::thread::spawn({
        let db = Arc::downgrade(&db);
        move || {
          let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();

          rt.block_on(async move {
            use tokio::time::*;
            let started = std::time::SystemTime::now();

            loop {
              sleep(Duration::from_mins(2)).await;

              let metrics = runtime_monitor.intervals();

              let now = std::time::SystemTime::now();
              if now.duration_since(started).unwrap_or_default() > Duration::from_mins(20) {
                println!("Test expired. Metrics = {metrics:?}");
                if let Some(db) = db.upgrade().and_then(|arc| arc.lock().take()) {
                  db.shutdown().unwrap();
                }
                panic!("test expired");
              }

              println!("metrics = {metrics:?}");
            }
          });
        }
      });

      // NOTE: `db.connection_uri()` returns rubbish for UDS, i.e. we need to construct our own uri.
      let pg_uri = format!(
        "postgresql://postgres@/template1?host={}",
        data_dir.main_db_path().to_string_lossy()
      );

      (Some(db), Some(pg_uri))
    } else {
      (None, None)
    };

    let TestStateOptions {
      config,
      mailer,
      json_schema_registry,
    } = options.unwrap_or_default();

    let json_schema_registry = Arc::new(parking_lot::RwLock::new(
      json_schema_registry
        .unwrap_or_else(|| trailbase_schema::registry::build_json_schema_registry(vec![]).unwrap()),
    ));

    let config = config.unwrap_or_else(test_config);
    update_json_schema_registry(&config.schemas, &json_schema_registry).unwrap();

    let logs_conn = crate::connection::init_logs_db(None)?;
    let session_conn = crate::connection::init_session_db(None)?;

    let connection_manager = ConnectionManager::new_for_test(
      data_dir.clone(),
      json_schema_registry.clone(),
      vec![],
      pg_uri.clone(),
    )
    .await;

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

    let config = Reactive::new(config);

    let record_apis = build_record_apis(
      connection_manager.clone(),
      config.derive(|c| c.record_apis.clone()),
    )
    .await;

    return Ok(AppState {
      state: Arc::new(InternalState {
        data_dir,
        start_time: std::time::SystemTime::now(),
        site_url: config.derive(|c| Arc::new(build_site_url(c).unwrap())),
        dev: true,
        demo: false,
        auth: config.derive_unchecked(|c| {
          Arc::new(AuthOptions::from_config(
            c.server.site_url.as_deref(),
            c.auth.clone(),
          ))
        }),
        jobs: config.derive_unchecked(|_c| Arc::new(JobRegistry::new())),
        mailer: mailer.map_or_else(
          || config.derive_unchecked(Mailer::new_from_config),
          |m| Reactive::new(m),
        ),
        config,
        json_schema_registry,
        conn: (*connection_manager.main_entry().connection).clone(),
        session_conn,
        logs_conn,
        connection_manager,
        jwt: crate::auth::jwt::test_jwt_helper(),
        record_apis: record_apis.clone(),
        subscription_manager: SubscriptionManager::new(record_apis),
        object_store,
        #[cfg(feature = "wasm")]
        wasm: crate::wasm::WasmState::default(),
        pg_uri,
        // NOTE: We gotta make sure `pg_db` is destroyed before the temp dir, otherwise it will
        // write new artifacts to the already deleted dir.
        test_cleanup: vec![Box::new(pg_db), Box::new(temp_dir)],
      }),
    });
  }
}

pub(crate) fn validate_path(path: Option<&PathBuf>) -> Result<(), InitError> {
  if let Some(path) = path
    && !std::fs::exists(path)?
  {
    return Err(InitError::CustomInit(format!("Path not found: {path:?}")));
  }
  return Ok(());
}

#[cfg(test)]
pub use test_utils::*;
