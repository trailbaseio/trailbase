use log::*;
use parking_lot::Mutex;
use std::path::PathBuf;
use thiserror::Error;

use crate::data_dir::DataDir;
use crate::migrations::{apply_logs_migrations, apply_main_migrations};

pub use trailbase_sqlite::Connection;

#[derive(Debug, Error)]
pub enum ConnectionError {
  #[error("SQLite ext error: {0}")]
  SqliteExtension(#[from] trailbase_extension::Error),
  #[error("Rusqlite error: {0}")]
  Rusqlite(#[from] rusqlite::Error),
  #[error("TB SQLite error: {0}")]
  TbSqlite(#[from] trailbase_sqlite::Error),
  #[error("Migration error: {0}")]
  Migration(#[from] trailbase_refinery::Error),
}

/// Initializes a new SQLite Connection with all the default extensions, migrations and settings
/// applied.
///
/// Returns a Connection and whether the DB was newly created..
pub fn init_main_db(
  data_dir: Option<&DataDir>,
  extensions: Option<Vec<PathBuf>>,
  attach: Option<Vec<(String, PathBuf)>>,
) -> Result<(Connection, bool), ConnectionError> {
  let new_db = Mutex::new(false);

  let main_path = data_dir.map(|d| d.main_db_path());
  let migrations_path = data_dir.map(|d| d.migrations_path());

  let conn = trailbase_sqlite::Connection::new(
    || -> Result<_, ConnectionError> {
      trailbase_schema::registry::try_init_schemas();

      let mut conn = trailbase_extension::connect_sqlite(main_path.clone(), extensions.clone())?;

      *(new_db.lock()) |= apply_main_migrations(&mut conn, migrations_path.clone())?;

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
  )?;

  if let Some(attach) = attach {
    for (schema_name, path) in attach {
      debug!("Attaching '{schema_name}': {path:?}");
      // FIXME: migrations for non-main databases.
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
