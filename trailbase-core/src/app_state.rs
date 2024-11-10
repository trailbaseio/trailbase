use libsql::Connection;
use log::*;
use object_store::ObjectStore;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use crate::auth::jwt::JwtHelper;
use crate::auth::oauth::providers::{ConfiguredOAuthProviders, OAuthProviderType};
use crate::config::proto::{Config, QueryApiConfig, RecordApiConfig};
use crate::config::{validate_config, write_config_and_vault_textproto};
use crate::constants::SITE_URL_DEFAULT;
use crate::data_dir::DataDir;
use crate::email::Mailer;
use crate::js::RuntimeHandle;
use crate::query::QueryApi;
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
  query_apis: Computed<Vec<(String, QueryApi)>, Config>,
  config: ValueNotifier<Config>,

  logs_conn: Connection,
  conn: Connection,

  jwt: JwtHelper,

  table_metadata: TableMetadataCache,

  #[allow(unused)]
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
  pub conn: Connection,
  pub logs_conn: Connection,
  pub jwt: JwtHelper,

  #[allow(unused)]
  pub tokio_runtime: Rc<tokio::runtime::Runtime>,
}

#[derive(Clone)]
pub struct AppState {
  state: Arc<InternalState>,
}

impl AppState {
  pub(crate) fn new(args: AppStateArgs) -> Self {
    let config = ValueNotifier::new(args.config);

    let table_metadata_clone = args.table_metadata.clone();
    let conn_clone0 = args.conn.clone();
    let conn_clone1 = args.conn.clone();

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
        record_apis: Computed::new(&config, move |c| {
          return c
            .record_apis
            .iter()
            .filter_map(|config| {
              match build_record_api(conn_clone0.clone(), &table_metadata_clone, config.clone()) {
                Ok(api) => Some((api.api_name().to_string(), api)),
                Err(err) => {
                  error!("{err}");
                  None
                }
              }
            })
            .collect::<Vec<_>>();
        }),
        query_apis: Computed::new(&config, move |c| {
          return c
            .query_apis
            .iter()
            .filter_map(
              |config| match build_query_api(conn_clone1.clone(), config.clone()) {
                Ok(api) => Some((api.api_name().to_string(), api)),
                Err(err) => {
                  error!("{err}");
                  None
                }
              },
            )
            .collect::<Vec<_>>();
        }),
        config,
        conn: args.conn.clone(),
        logs_conn: args.logs_conn,
        jwt: args.jwt,
        table_metadata: args.table_metadata,

        runtime: RuntimeHandle::new(),

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

  pub fn conn(&self) -> &Connection {
    return &self.state.conn;
  }

  pub(crate) fn user_conn(&self) -> &Connection {
    return &self.state.conn;
  }

  pub(crate) fn logs_conn(&self) -> &Connection {
    return &self.state.logs_conn;
  }

  pub(crate) fn table_metadata(&self) -> &TableMetadataCache {
    return &self.state.table_metadata;
  }

  pub async fn refresh_table_cache(&self) -> Result<(), crate::table_metadata::TableLookupError> {
    self.table_metadata().invalidate_all().await
  }

  pub(crate) fn objectstore(
    &self,
  ) -> Result<Box<dyn ObjectStore + Send + Sync>, object_store::Error> {
    // FIXME: We should probably have a long-lived store on AppState.
    return Ok(Box::new(
      object_store::local::LocalFileSystem::new_with_prefix(self.data_dir().uploads_path())?,
    ));
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

  pub(crate) fn lookup_query_api(&self, name: &str) -> Option<QueryApi> {
    for (query_api_name, query_api) in self.state.query_apis.load().iter() {
      if query_api_name == name {
        return Some(query_api.clone());
      }
    }
    return None;
  }

  pub fn get_config(&self) -> Config {
    return (*self.state.config.load_full()).clone();
  }

  pub(crate) fn access_config<F, T>(&self, f: F) -> T
  where
    F: Fn(&Config) -> T,
  {
    return f(&self.state.config.load());
  }

  pub(crate) async fn validate_and_update_config(
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

  pub(crate) fn script_runtime(&self) -> &RuntimeHandle {
    return &self.state.runtime;
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

  let temp_dir = temp_dir::TempDir::new()?;
  tokio::fs::create_dir_all(temp_dir.child("uploads")).await?;

  let main_conn = {
    let conn = trailbase_sqlite::connect_sqlite(None, None).await?;
    apply_user_migrations(conn.clone()).await?;
    let _new_db = apply_main_migrations(conn.clone(), None).await?;

    conn
  };

  let logs_conn = {
    let conn = trailbase_sqlite::connect_sqlite(None, None).await?;
    apply_logs_migrations(conn.clone()).await?;
    conn
  };

  let table_metadata = TableMetadataCache::new(main_conn.clone()).await?;

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

  let main_conn_clone0 = main_conn.clone();
  let main_conn_clone1 = main_conn.clone();
  let table_metadata_clone = table_metadata.clone();

  return Ok(AppState {
    state: Arc::new(InternalState {
      data_dir: DataDir(temp_dir.path().to_path_buf()),
      public_dir: None,
      dev: true,
      oauth: Computed::new(&config, |c| {
        ConfiguredOAuthProviders::from_config(c.auth.clone()).unwrap()
      }),
      mailer: build_mailer(&config, options.and_then(|o| o.mailer)),
      record_apis: Computed::new(&config, move |c| {
        return c
          .record_apis
          .iter()
          .filter_map(|config| {
            let api = build_record_api(
              main_conn_clone0.clone(),
              &table_metadata_clone,
              config.clone(),
            )
            .unwrap();

            return Some((api.api_name().to_string(), api));
          })
          .collect::<Vec<_>>();
      }),
      query_apis: Computed::new(&config, move |c| {
        return c
          .query_apis
          .iter()
          .filter_map(|config| {
            let api = build_query_api(main_conn_clone1.clone(), config.clone()).unwrap();

            return Some((api.api_name().to_string(), api));
          })
          .collect::<Vec<_>>();
      }),
      config,
      conn: main_conn.clone(),
      logs_conn,
      jwt: jwt::test_jwt_helper(),
      table_metadata,
      runtime: RuntimeHandle::new(),
      cleanup: vec![Box::new(temp_dir)],
    }),
  });
}

fn build_record_api(
  conn: libsql::Connection,
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

fn build_query_api(conn: libsql::Connection, config: QueryApiConfig) -> Result<QueryApi, String> {
  // TODO: Check virtual table exists
  return QueryApi::from(conn, config);
}
