use log::*;
use std::path::PathBuf;
use thiserror::Error;

use crate::app_state::{AppState, AppStateArgs, build_objectstore};
use crate::auth::jwt::{JwtHelper, JwtHelperError};
use crate::config::load_or_init_config_textproto;
use crate::constants::USER_TABLE;
use crate::metadata::load_or_init_metadata_textproto;
use crate::rand::generate_random_string;
use crate::schema_metadata::SchemaMetadataCache;
use crate::server::DataDir;

#[derive(Debug, Error)]
pub enum InitError {
  #[error("SQLite error: {0}")]
  Sqlite(#[from] trailbase_sqlite::Error),
  #[error("Connection error: {0}")]
  Connection(#[from] crate::connection::ConnectionError),
  #[error("IO error: {0}")]
  IO(#[from] std::io::Error),
  #[error("Config error: {0}")]
  Config(#[from] crate::config::ConfigError),
  #[error("JwtHelper error: {0}")]
  JwtHelper(#[from] JwtHelperError),
  #[error("CreateAdmin error: {0}")]
  CreateAdmin(String),
  #[error("Custom initializer error: {0}")]
  CustomInit(String),
  #[error("Table error: {0}")]
  TableError(#[from] crate::schema_metadata::SchemaLookupError),
  #[error("Schema error: {0}")]
  SchemaError(#[from] trailbase_schema::Error),
  #[error("Script error: {0}")]
  ScriptError(String),
  #[error("ObjectStore error: {0}")]
  ObjectStore(#[from] object_store::Error),
  #[error("Auth error: {0}")]
  Auth(#[from] crate::auth::AuthError),
}

#[derive(Default)]
pub struct InitArgs {
  pub data_dir: DataDir,
  pub public_url: Option<url::Url>,
  pub public_dir: Option<PathBuf>,
  pub runtime_root_fs: Option<PathBuf>,
  pub geoip_db_path: Option<PathBuf>,

  pub address: String,
  pub dev: bool,
  pub demo: bool,
  pub runtime_threads: Option<usize>,
}

pub async fn init_app_state(args: InitArgs) -> Result<(bool, AppState), InitError> {
  // First create directory structure.
  args.data_dir.ensure_directory_structure().await?;

  // Then open or init new databases.
  let logs_conn = crate::connection::init_logs_db(Some(&args.data_dir))?;

  // WIP: Allow multiple databases.
  //
  // Open or init the main db connection. Note that we derive whether a new DB was initialized
  // based on whether the V1 migration had to be applied. Should be fairly robust.
  let paths = std::fs::read_dir(args.data_dir.data_path())?;
  let extra_databases: Vec<(String, PathBuf)> = paths
    .filter_map(|entry: Result<std::fs::DirEntry, _>| {
      if let Ok(entry) = entry {
        let path = entry.path();
        if let (Some(stem), Some(ext)) = (path.file_stem(), path.extension()) {
          if ext != "db" {
            return None;
          }

          if stem != "main" && stem != "logs" {
            return Some((stem.to_string_lossy().to_string(), path.to_path_buf()));
          }
        }
      }
      return None;
    })
    .collect();

  let (conn, new_db) =
    crate::connection::init_main_db(Some(&args.data_dir), Some(extra_databases))?;

  let mut schema_metadata = SchemaMetadataCache::new(&conn).await?;

  // Read config or write default one.
  let config = load_or_init_config_textproto(&args.data_dir, &schema_metadata).await?;

  if !config.schemas.is_empty() {
    let schemas: Vec<_> = config
      .schemas
      .iter()
      .map(|s| {
        // Any panics here should be captured by config validation during load above.
        let (Some(name), Some(schema)) = (&s.name, &s.schema) else {
          panic!("Schema config invalid entry: {s:?}");
        };

        return (
          name.clone(),
          serde_json::from_str(schema).unwrap_or_else(|err| {
            panic!("Invalid schema definition for '{name}': {err}");
          }),
        );
      })
      .collect();

    debug!(
      "Initializing JSON schemas from config: {schemas:?}",
      schemas = schemas.iter().map(|(name, _)| name.as_str())
    );

    trailbase_schema::registry::set_user_schemas(schemas)?;

    // NOTE: We must reload the table schema metadata after registering new schemas. This is a
    // work-around because config validation currently depends on SchemaMetadataCache and thus the
    // JSON schema registry. It would be cleaner to build SchemaMatadataCache only after
    // registering custom schemas and validating the config only against plain TABLE/VIEW
    // metadata.
    schema_metadata = SchemaMetadataCache::new(&conn).await?;
  }

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

  let object_store = build_objectstore(&args.data_dir, config.server.s3_storage_config.as_ref())?;

  // Write out the latest .js/.d.ts runtime files.
  #[cfg(feature = "v8")]
  trailbase_js::runtime::write_js_runtime_files(args.data_dir.root()).await;

  let app_state = AppState::new(AppStateArgs {
    data_dir: args.data_dir.clone(),
    public_url: args.public_url,
    public_dir: args.public_dir,
    runtime_root_fs: args.runtime_root_fs,
    dev: args.dev,
    demo: args.demo,
    schema_metadata,
    config,
    conn,
    logs_conn,
    jwt,
    object_store,
    runtime_threads: args.runtime_threads,
  });

  if new_db {
    let num_admins: i64 = app_state
      .user_conn()
      .read_query_row_f(
        format!("SELECT COUNT(*) FROM {USER_TABLE} WHERE admin = TRUE"),
        (),
        |row| row.get(0),
      )
      .await?
      .unwrap_or(0);

    if num_admins == 0 {
      let email = "admin@localhost";
      let password = generate_random_string(20);
      let hashed_password = crate::auth::password::hash_password(&password)?;

      app_state
        .user_conn()
        .execute(
          format!(
            r#"
              INSERT INTO {USER_TABLE}
                (email, password_hash, verified, admin)
              VALUES
                (?1, ?2, TRUE, TRUE)
            "#
          ),
          trailbase_sqlite::params!(email.to_string(), hashed_password),
        )
        .await?;

      info!(
        "{}",
        indoc::formatdoc!(
          r#"
          Created new admin user:
              email: '{email}'
              password: '{password}'
        "#
        )
      );
    }
  }

  if cfg!(debug_assertions) {
    let text_config = app_state.get_config().to_text()?;
    debug!("Config: {text_config}");
  }

  return Ok((new_db, app_state));
}
