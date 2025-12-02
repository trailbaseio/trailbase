use log::*;
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

use crate::app_state::{AppState, AppStateArgs, build_objectstore, update_json_schema_registry};
use crate::auth::jwt::{JwtHelper, JwtHelperError};
use crate::config::load_or_init_config_textproto;
use crate::connection::AttachExtraDatabases;
use crate::constants::USER_TABLE;
use crate::metadata::load_or_init_metadata_textproto;
use crate::rand::generate_random_string;
use crate::schema_metadata::{
  build_connection_metadata_and_install_file_deletion_triggers, lookup_and_parse_all_table_schemas,
  lookup_and_parse_all_view_schemas,
};
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

  let json_schema_registry = Arc::new(RwLock::new(
    trailbase_schema::registry::build_json_schema_registry(vec![])?,
  ));

  let (conn, new_db) = init_connection(
    &args.data_dir,
    args.runtime_root_fs.as_deref(),
    json_schema_registry.clone(),
    args.dev,
  )?;

  let tables = lookup_and_parse_all_table_schemas(&conn).await?;
  let views = lookup_and_parse_all_view_schemas(&conn, &tables).await?;

  // Read config or write default one. Ensures config is validated.
  let config = {
    let config = load_or_init_config_textproto(&args.data_dir, &tables, &views).await?;
    update_json_schema_registry(&config, &json_schema_registry)?;
    config
  };

  // Load the `<depot>/metadata.textproto`.
  let _metadata = load_or_init_metadata_textproto(&args.data_dir).await?;

  let connection_metadata = build_connection_metadata_and_install_file_deletion_triggers(
    &conn,
    tables,
    views,
    &json_schema_registry,
  )
  .await?;

  let jwt = JwtHelper::init_from_path(&args.data_dir).await?;

  // Init geoip if present.
  let geoip_db_path = args
    .geoip_db_path
    .unwrap_or_else(|| args.data_dir.root().join("GeoLite2-Country.mmdb"));
  if let Err(err) = trailbase_extension::geoip::load_geoip_db(geoip_db_path.clone()) {
    debug!("Failed to load maxmind geoip DB '{geoip_db_path:?}': {err}");
  }

  let object_store = build_objectstore(&args.data_dir, config.server.s3_storage_config.as_ref())?;

  let app_state = AppState::new(AppStateArgs {
    data_dir: args.data_dir.clone(),
    public_url: args.public_url,
    public_dir: args.public_dir,
    runtime_root_fs: args.runtime_root_fs,
    dev: args.dev,
    demo: args.demo,
    connection_metadata,
    config,
    json_schema_registry,
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

pub(crate) fn init_connection(
  data_dir: &DataDir,
  runtime_root_fs: Option<&Path>,
  json_schema_registry: Arc<RwLock<trailbase_schema::registry::JsonSchemaRegistry>>,
  dev: bool,
) -> Result<(trailbase_sqlite::Connection, bool), InitError> {
  // WIP: Allow multiple databases.
  //
  // Open or init the main db connection. Note that we derive whether a new DB was initialized
  // based on whether the V1 migration had to be applied. Should be fairly robust.
  let extra_databases: Vec<AttachExtraDatabases> = std::fs::read_dir(data_dir.data_path())?
    .filter_map(|entry: Result<std::fs::DirEntry, _>| {
      if let Ok(entry) = entry {
        let path = entry.path();
        if let (Some(stem), Some(ext)) = (path.file_stem(), path.extension()) {
          if ext != "db" {
            return None;
          }

          if stem != "main" && stem != "logs" {
            return Some(AttachExtraDatabases {
              schema_name: stem.to_string_lossy().to_string(),
              path: path.to_path_buf(),
            });
          }
        }
      }
      return None;
    })
    .collect();

  // SQLite supports only up to 125 DBs per connection: https://sqlite.org/limits.html.
  if extra_databases.len() > 124 {
    return Err(InitError::CustomInit("Too many databases".into()));
  }

  let sync_wasm_runtimes = crate::wasm::build_sync_wasm_runtimes_for_components(
    data_dir.root().join("wasm"),
    runtime_root_fs,
    dev,
  )
  .map_err(|err| InitError::CustomInit(err.to_string()))?;

  // NOTE: We're injecting a WASM runtime to make custom functions available.
  let (conn, new_db) = crate::connection::init_main_db(
    Some(data_dir),
    Some(json_schema_registry.clone()),
    Some(extra_databases),
    sync_wasm_runtimes,
  )?;

  return Ok((conn, new_db));
}
