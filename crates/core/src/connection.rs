use log::*;
use parking_lot::{Mutex, RwLock};
use std::collections::{BTreeSet, HashMap};
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

pub struct AttachedDatabase {
  pub schema_name: String,
  pub path: PathBuf,
}

impl AttachedDatabase {
  pub fn from_data_dir(data_dir: &DataDir, name: impl std::string::ToString) -> Self {
    let name = name.to_string();
    return AttachedDatabase {
      path: data_dir.data_path().join(format!("{name}.db")),
      schema_name: name,
    };
  }
}

#[derive(Hash, PartialEq, Eq)]
struct ConnectionKey {
  main: bool,
  attached_databases: BTreeSet<String>,
}

// A manager for multi-DB SQLite connections.
//
// NOTE: Performance-wise it's beneficial to share Connections to benefit from its internal locking
// instead of relying on SQLite's own file locking.
#[derive(Clone)]
pub(crate) struct ConnectionManager {
  data_dir: DataDir,
  json_schema_registry: Arc<RwLock<trailbase_schema::registry::JsonSchemaRegistry>>,
  sqlite_function_runtimes: Vec<SqliteFunctionRuntime>,

  // Cache of existing Sqlite connections:
  main: Arc<Connection>,
  // NOTE: we should probably be smarter and keep connections alive for longer, e.g. TTL or LRU,
  // rather than fading them out so quickly.
  connections: Arc<RwLock<HashMap<ConnectionKey, std::sync::Weak<Connection>>>>,
}

impl ConnectionManager {
  pub(crate) fn new(
    main_connection: Connection,
    data_dir: DataDir,
    json_schema_registry: Arc<RwLock<trailbase_schema::registry::JsonSchemaRegistry>>,
    sqlite_function_runtimes: Vec<SqliteFunctionRuntime>,
  ) -> Self {
    return Self {
      data_dir,
      json_schema_registry,
      sqlite_function_runtimes,
      main: Arc::new(main_connection),
      connections: Arc::new(RwLock::new(HashMap::new())),
    };
  }

  pub(crate) fn main(&self) -> &Arc<Connection> {
    return &self.main;
  }

  pub(crate) fn get(
    &self,
    main: bool,
    attached_databases: Option<BTreeSet<String>>,
  ) -> Result<Arc<Connection>, ConnectionError> {
    if main && attached_databases.is_none() {
      return Ok(self.main.clone());
    }

    let key = ConnectionKey {
      main,
      attached_databases: attached_databases.unwrap_or_default(),
    };

    let mut lock = self.connections.upgradable_read();
    if let Some(weak) = lock.get(&key) {
      if let Some(conn) = weak.upgrade() {
        return Ok(conn);
      } else {
        lock.with_upgraded(|lock| lock.remove(&key));
      }
    }

    let conn = self.build(main, Some(&key.attached_databases))?;

    lock.with_upgraded(|lock| {
      lock.insert(key, Arc::downgrade(&conn));
    });

    return Ok(conn);
  }

  pub(crate) fn build(
    &self,
    main: bool,
    attached_databases: Option<&BTreeSet<String>>,
  ) -> Result<Arc<Connection>, ConnectionError> {
    let attach = if let Some(attached_databases) = attached_databases {
      // SQLite supports only up to 125 DBs per connection: https://sqlite.org/limits.html.
      if attached_databases.len() > 124 {
        return Err(ConnectionError::Other("Too many databases".into()));
      }

      attached_databases
        .iter()
        .map(|name| AttachedDatabase::from_data_dir(&self.data_dir, name))
        .collect()
    } else {
      vec![]
    };

    let (conn, _new_db) = crate::connection::init_main_db_impl(
      if main { Some(&self.data_dir) } else { None },
      Some(self.json_schema_registry.clone()),
      attach,
      self.sqlite_function_runtimes.clone(),
      main,
    )?;

    return Ok(Arc::new(conn));
  }
}

/// Initializes a new SQLite Connection with all the default extensions, migrations and settings
/// applied.
///
/// Returns a Connection and whether the DB was newly created..
pub fn init_main_db(
  data_dir: Option<&DataDir>,
  json_registry: Option<Arc<RwLock<JsonSchemaRegistry>>>,
  attached_databases: Vec<AttachedDatabase>,
  runtimes: Vec<SqliteFunctionRuntime>,
) -> Result<(Connection, bool), ConnectionError> {
  // SQLite supports only up to 125 DBs per connection: https://sqlite.org/limits.html.
  if attached_databases.len() > 124 {
    return Err(ConnectionError::Other("Too many databases".into()));
  }

  return init_main_db_impl(data_dir, json_registry, attached_databases, runtimes, true);
}

fn init_main_db_impl(
  data_dir: Option<&DataDir>,
  json_registry: Option<Arc<RwLock<JsonSchemaRegistry>>>,
  attach: Vec<AttachedDatabase>,
  runtimes: Vec<SqliteFunctionRuntime>,
  main_migrations: bool,
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
    let migrations_path = migrations_path.clone();

    trailbase_sqlite::Connection::new(
      move || -> Result<_, ConnectionError> {
        let mut conn =
          trailbase_extension::connect_sqlite(main_path.clone(), json_registry.clone())?;

        if main_migrations {
          *(new_db.lock()) |= apply_main_migrations(&mut conn, migrations_path.clone())?;
        }

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

  for AttachedDatabase { schema_name, path } in attach {
    debug!("Attaching '{schema_name}': {path:?}");

    if let Some(ref migrations_path) = migrations_path {
      let mut secondary =
        connect_rusqlite_without_default_extensions_and_schemas(Some(path.clone()))?;

      let migrations = vec![load_sql_migrations(
        migrations_path.join(&schema_name),
        false,
      )?];
      apply_migrations(&schema_name, &mut secondary, migrations)?;
    }

    conn.attach(&path.to_string_lossy(), &schema_name)?;
  }

  // NOTE: We could consider larger memory maps and caches for the main database.
  // Should be driven by benchmarks.
  // conn.pragma_update(None, "mmap_size", 268435456)?;
  // conn.pragma_update(None, "cache_size", -32768)?; // 32MB

  return Ok((conn, *new_db.lock()));
}

pub(super) fn init_logs_db(data_dir: Option<&DataDir>) -> Result<Connection, ConnectionError> {
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
