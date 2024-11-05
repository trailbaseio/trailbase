use libsql::Connection;
use log::*;
use std::path::PathBuf;
use thiserror::Error;
use trailbase_sqlite::{connect_sqlite, query_one_row};

use crate::app_state::AppState;
use crate::auth::jwt::{JwtHelper, JwtHelperError};
use crate::config::load_or_init_config_textproto;
use crate::constants::USER_TABLE;
use crate::migrations::{apply_logs_migrations, apply_main_migrations};
use crate::rand::generate_random_string;
use crate::server::DataDir;
use crate::table_metadata::TableMetadataCache;

#[derive(Debug, Error)]
pub enum InitError {
  #[error("Libsql error: {0}")]
  Libsql(#[from] libsql::Error),
  #[error("DB Migration error: {0}")]
  Migration(#[from] refinery::Error),
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
  TableError(#[from] crate::table_metadata::TableLookupError),
  #[error("Schema error: {0}")]
  SchemaError(#[from] trailbase_sqlite::schema::SchemaError),
}

pub async fn init_app_state(
  data_dir: DataDir,
  public_dir: Option<PathBuf>,
  dev: bool,
) -> Result<(bool, AppState), InitError> {
  // First create directory structure.
  data_dir.ensure_directory_structure().await?;

  // Then open or init new databases.
  let logs_conn = init_logs_db(&data_dir).await?;

  // Open or init the main db. Note that we derive whether a new DB was initialized based on
  // whether the V1 migration had to be applied. Should be fairly robust.
  let (main_conn, new_db) = {
    let conn = connect_sqlite(Some(data_dir.main_db_path()), None).await?;
    let new_db = apply_main_migrations(conn.clone(), Some(data_dir.migrations_path())).await?;

    (conn, new_db)
  };

  let table_metadata = TableMetadataCache::new(main_conn.clone()).await?;

  // Read config or write default one.
  let config = load_or_init_config_textproto(&data_dir, &table_metadata).await?;

  debug!("Initializing JSON schemas from config");
  trailbase_sqlite::set_user_schemas(
    config
      .schemas
      .iter()
      .filter_map(|s| {
        let Some(ref name) = s.name else {
          warn!("Schema config entry missing name: {s:?}");
          return None;
        };

        let Some(ref schema) = s.schema else {
          warn!("Schema config entry missing schema: {s:?}");
          return None;
        };

        let json = match serde_json::from_str(schema) {
          Ok(json) => json,
          Err(err) => {
            error!("Invalid schema config entry for '{name}': {err}");
            return None;
          }
        };

        return Some((name.clone(), json));
      })
      .collect(),
  )?;

  let jwt = JwtHelper::init_from_path(&data_dir).await?;

  // Init geoip if present.
  let geoip_db_path = data_dir.root().join("GeoLite2-Country.mmdb");
  if let Err(err) = trailbase_sqlite::load_geoip_db(geoip_db_path.clone()) {
    debug!("Failed to load maxmind geoip DB '{geoip_db_path:?}': {err}");
  }

  let app_state = AppState::new(
    data_dir.clone(),
    public_dir,
    dev,
    table_metadata,
    config,
    main_conn.clone(),
    logs_conn,
    jwt,
  );

  if new_db {
    let num_admins: i64 = query_one_row(
      app_state.user_conn(),
      &format!("SELECT COUNT(*) FROM {USER_TABLE} WHERE admin = TRUE"),
      (),
    )
    .await?
    .get(0)?;

    if num_admins == 0 {
      let email = "admin@localhost".to_string();
      let password = generate_random_string(20);

      app_state
        .user_conn()
        .execute(
          &format!(
            r#"
        INSERT INTO {USER_TABLE}
          (email, password_hash, verified, admin)
        VALUES
          ('{email}', (hash_password('{password}')), TRUE, TRUE);
        INSERT INTO
        "#
          ),
          libsql::params!(),
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

async fn init_logs_db(data_dir: &DataDir) -> Result<Connection, InitError> {
  let conn = connect_sqlite(data_dir.logs_db_path().into(), None).await?;

  // Turn off secure_deletions, i.e. don't wipe the memory with zeros.
  conn
    .query("PRAGMA secure_delete = FALSE", ())
    .await
    .unwrap();

  // Sync less often
  conn.execute("PRAGMA synchronous = 1", ()).await.unwrap();

  apply_logs_migrations(conn.clone()).await?;
  return Ok(conn);
}
