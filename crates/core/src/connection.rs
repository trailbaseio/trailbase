use log::*;
use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use trailbase_extension::jsonschema::JsonSchemaRegistry;

use crate::data_dir::DataDir;
use crate::migrations::{
  apply_logs_migrations, apply_main_migrations, apply_migrations, load_sql_migrations,
};
use crate::wasm::SqliteFunctionRuntime;

pub use trailbase_sqlite::Connection;

#[derive(Debug, Error)]
pub enum ConnectionError {
  #[error("SQLite ext: {0}")]
  SqliteExtension(#[from] trailbase_extension::Error),
  #[error("Rusqlite: {0}")]
  Rusqlite(#[from] rusqlite::Error),
  #[error("TB SQLite: {0}")]
  TbSqlite(#[from] trailbase_sqlite::Error),
  #[error("Migration: {0}")]
  Migration(#[from] trailbase_refinery::Error),
  #[error("Other: {0}")]
  Other(String),
}

pub struct AttachExtraDatabases {
  pub schema_name: String,
  pub path: PathBuf,
}

/// Initializes a new SQLite Connection with all the default extensions, migrations and settings
/// applied.
///
/// Returns a Connection and whether the DB was newly created..
pub fn init_main_db(
  data_dir: Option<&DataDir>,
  json_registry: Option<Arc<RwLock<JsonSchemaRegistry>>>,
  attach: Option<Vec<AttachExtraDatabases>>,
  runtimes: Vec<SqliteFunctionRuntime>,
) -> Result<(Connection, bool), ConnectionError> {
  let main_path = data_dir.map(|d| d.main_db_path());
  let migrations_path = data_dir.map(|d| d.migrations_path());

  #[cfg(feature = "wasm")]
  let sqlite_functions: Vec<_> = runtimes
    .into_iter()
    .map(|rt| -> Result<_, trailbase_wasm_runtime_host::Error> {
      let functions =
        rt.initialize_sqlite_functions(trailbase_wasm_runtime_host::InitArgs { version: None })?;
      return Ok((rt, functions));
    })
    .collect::<Result<Vec<_>, _>>()
    .map_err(|err| return ConnectionError::Other(err.to_string()))?;

  let new_db = Arc::new(Mutex::new(false));
  let conn = {
    let new_db = new_db.clone();

    trailbase_sqlite::Connection::new(
      move || -> Result<_, ConnectionError> {
        let mut conn =
          trailbase_extension::connect_sqlite(main_path.clone(), json_registry.clone())?;

        *(new_db.lock()) |= apply_main_migrations(&mut conn, migrations_path.clone())?;

        #[cfg(feature = "wasm")]
        for (rt, functions) in &sqlite_functions {
          trailbase_wasm_runtime_host::functions::setup_connection(&conn, rt, functions)
            .expect("startup");
        }

        return Ok(conn);
      },
      Some(trailbase_sqlite::connection::Options {
        n_read_threads: match (data_dir, std::thread::available_parallelism()) {
          (None, _) => 0,
          (Some(_), Ok(n)) => n.get().clamp(2, 4),
          (Some(_), Err(_)) => 4,
        },
        ..Default::default()
      }),
    )?
  };

  if let Some(attach) = attach {
    for AttachExtraDatabases { schema_name, path } in attach {
      debug!("Attaching '{schema_name}': {path:?}");

      // FIXME: Extra DBs should probably be config driven, rather than discovered. main is created
      // when missing in a new environment. So should others.
      let mut secondary =
        connect_rusqlite_without_default_extensions_and_schemas(Some(path.clone()))?;
      let migrations = vec![load_sql_migrations(path.clone(), false)?];
      apply_migrations(&schema_name, &mut secondary, migrations)?;

      conn.attach(&path.to_string_lossy(), &schema_name)?;
    }
  }

  // NOTE: We could consider larger memory maps and caches for the main database.
  // Should be driven by benchmarks.
  // conn.pragma_update(None, "mmap_size", 268435456)?;
  // conn.pragma_update(None, "cache_size", -32768)?; // 32MB

  return Ok((conn, *new_db.lock()));
}

pub(crate) fn init_logs_db(data_dir: Option<&DataDir>) -> Result<Connection, ConnectionError> {
  let path = data_dir.map(|d| d.logs_db_path());

  return trailbase_sqlite::Connection::new(
    || -> Result<_, ConnectionError> {
      // NOTE: The logs db needs the trailbase extensions for the maxminddb geoip lookup.
      let mut conn = trailbase_extension::sqlite3_extension_init(
        connect_rusqlite_without_default_extensions_and_schemas(path.clone())?,
        None,
      )?;

      // Turn off secure_deletions, i.e. don't wipe the memory with zeros.
      conn.pragma_update(None, "secure_delete", "FALSE")?;

      apply_logs_migrations(&mut conn)?;
      return Ok(conn);
    },
    None,
  );
}

pub(crate) fn connect_rusqlite_without_default_extensions_and_schemas(
  path: Option<PathBuf>,
) -> Result<rusqlite::Connection, rusqlite::Error> {
  let conn = if let Some(p) = path {
    use rusqlite::OpenFlags;
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
      | OpenFlags::SQLITE_OPEN_CREATE
      | OpenFlags::SQLITE_OPEN_NO_MUTEX;

    rusqlite::Connection::open_with_flags(p, flags)?
  } else {
    rusqlite::Connection::open_in_memory()?
  };

  trailbase_extension::apply_default_pragmas(&conn)?;

  // Initial optimize.
  conn.pragma_update(None, "optimize", "0x10002")?;

  return Ok(conn);
}
