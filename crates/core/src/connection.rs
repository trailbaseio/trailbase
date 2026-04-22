use log::*;
use parking_lot::{Mutex, RwLock};
use quick_cache::sync::GuardResult;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use trailbase_extension::jsonschema::JsonSchemaRegistry;
use trailbase_schema::metadata::ConnectionMetadata;

pub use trailbase_sqlite::Connection;

use crate::data_dir::DataDir;
use crate::migrations::{
  apply_base_migrations, apply_logs_migrations, apply_main_migrations, apply_session_migrations,
};
use crate::schema_metadata::{build_metadata_async, build_metadata_sync};
use crate::wasm::{SqliteFunctions, SqliteStore};

#[derive(Debug, Error)]
pub enum ConnectionError {
  #[error("SQLite ext: {0}")]
  SqliteExtension(#[from] trailbase_extension::Error),
  #[error("Schema: {0}")]
  Schema(#[from] crate::schema_metadata::SchemaLookupError),
  #[error("Rusqlite: {0}")]
  Rusqlite(#[from] rusqlite::Error),
  #[error("TB SQLite: {0}")]
  TbSqlite(#[from] trailbase_sqlite::Error),
  #[error("Migration: {0}")]
  Migration(#[from] trailbase_refinery::Error),
  #[error("MissingMetadata")]
  MissingMetadata,
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

#[derive(Clone, Hash, PartialEq, Eq)]
struct ConnectionKey {
  main: bool,
  attached_databases: BTreeSet<String>,
}

#[derive(Clone)]
pub struct ConnectionEntry {
  pub connection: Arc<Connection>,
  pub metadata: Arc<ConnectionMetadata>,
}

struct ConnectionManagerState {
  // Properties retained for initializing new connections.
  data_dir: DataDir,
  json_schema_registry: Arc<RwLock<trailbase_schema::registry::JsonSchemaRegistry>>,
  sqlite_function_runtimes: Vec<(SqliteStore, SqliteFunctions)>,

  // Properties for caching connections:
  main: RwLock<ConnectionEntry>,
  connections: quick_cache::sync::Cache<ConnectionKey, ConnectionEntry>,
}

// A manager for multi-DB SQLite connections.
//
// NOTE: Performance-wise it's beneficial to share Connections to benefit from its internal locking
// instead of relying on SQLite's own file locking.
#[derive(Clone)]
pub struct ConnectionManager {
  state: Arc<ConnectionManagerState>,
}

impl ConnectionManager {
  pub(crate) fn new(
    data_dir: DataDir,
    json_schema_registry: Arc<RwLock<trailbase_schema::registry::JsonSchemaRegistry>>,
    sqlite_function_runtimes: Vec<(SqliteStore, SqliteFunctions)>,
  ) -> Result<(Self, bool), ConnectionError> {
    let (main_conn, main_metadata, new_db) = init_main_db_impl(
      Some(&data_dir),
      Some(json_schema_registry.clone()),
      vec![],
      sqlite_function_runtimes.clone(),
      true,
    )?;

    return Ok((
      Self {
        state: Arc::new(ConnectionManagerState {
          data_dir,
          json_schema_registry,
          sqlite_function_runtimes,
          main: RwLock::new(ConnectionEntry {
            connection: Arc::new(main_conn),
            metadata: Arc::new(main_metadata),
          }),
          connections: quick_cache::sync::Cache::new(256),
        }),
      },
      new_db,
    ));
  }

  #[cfg(test)]
  pub(crate) fn new_for_test(
    data_dir: DataDir,
    json_schema_registry: Arc<RwLock<trailbase_schema::registry::JsonSchemaRegistry>>,
    sqlite_function_runtimes: Vec<(SqliteStore, SqliteFunctions)>,
  ) -> Self {
    let (main_conn, main_metadata, new_db) = init_main_db_impl(
      None,
      Some(json_schema_registry.clone()),
      vec![],
      sqlite_function_runtimes.clone(),
      true,
    )
    .unwrap();
    assert!(new_db);

    return Self {
      state: Arc::new(ConnectionManagerState {
        data_dir,
        json_schema_registry,
        sqlite_function_runtimes,
        main: RwLock::new(ConnectionEntry {
          connection: Arc::new(main_conn),
          metadata: Arc::new(main_metadata),
        }),
        connections: quick_cache::sync::Cache::new(256),
      }),
    };
  }

  pub fn main_entry(&self) -> ConnectionEntry {
    return self.state.main.read().clone();
  }

  pub fn get_entry(
    &self,
    main: bool,
    attached_databases: Option<BTreeSet<String>>,
  ) -> Result<ConnectionEntry, ConnectionError> {
    if main && attached_databases.is_none() {
      return Ok(self.state.main.read().clone());
    }

    let key = ConnectionKey {
      main,
      attached_databases: attached_databases.unwrap_or_default(),
    };

    return match self.state.connections.get_value_or_guard(&key, None) {
      GuardResult::Value(entry) => Ok(entry.clone()),
      GuardResult::Guard(placeholder) => {
        let entry = self.build(main, Some(&key.attached_databases))?;
        let _ = placeholder.insert(entry.clone());
        Ok(entry)
      }
      GuardResult::Timeout => {
        return Err(ConnectionError::Other("Timeout".into()));
      }
    };
  }

  pub fn get_entry_for_qn(
    &self,
    name: &trailbase_schema::QualifiedName,
  ) -> Result<ConnectionEntry, ConnectionError> {
    if let Some(ref db) = name.database_schema
      && db != "main"
    {
      // QUESTION: Should we disallow access to "logs", "auth", etc? Currently, this is not
      // exposed to WASM, i.e. there's no sanctioned way to interact with this.
      return self.get_entry(false, Some([db.to_string()].into()));
    }

    return Ok(self.main_entry());
  }

  pub(crate) fn build(
    &self,
    mut main: bool,
    attached_databases: Option<&BTreeSet<String>>,
  ) -> Result<ConnectionEntry, ConnectionError> {
    #[cfg(test)]
    if main && attached_databases.is_none() {
      return Ok(self.state.main.read().clone());
    }

    let attach = if let Some(attached_databases) = attached_databases {
      // SQLite supports only up to 125 DBs per connection: https://sqlite.org/limits.html.
      if attached_databases.len() > 124 {
        return Err(ConnectionError::Other("Too many databases".into()));
      }

      attached_databases
        .iter()
        .flat_map(|name| {
          if name != "main" {
            Some(AttachedDatabase::from_data_dir(&self.state.data_dir, name))
          } else {
            main = true;
            None
          }
        })
        .collect()
    } else {
      vec![]
    };

    let (conn, metadata, _new_db) = init_main_db_impl(
      Some(&self.state.data_dir),
      Some(self.state.json_schema_registry.clone()),
      attach,
      self.state.sqlite_function_runtimes.clone(),
      main,
    )?;

    return Ok(ConnectionEntry {
      connection: Arc::new(conn),
      metadata: Arc::new(metadata),
    });
  }

  // Updates connection metadata for cached connections.
  pub(crate) async fn rebuild_metadata(&self) -> Result<(), ConnectionError> {
    // Main
    {
      let new_metadata = Arc::new({
        let conn = self.state.main.read().connection.clone();
        build_metadata_async(&conn, &self.state.json_schema_registry).await?
      });

      self.state.main.write().metadata = new_metadata;
    }

    // Others:
    for (key, entry) in self.state.connections.iter() {
      let new_metadata =
        Arc::new(build_metadata_async(&entry.connection, &self.state.json_schema_registry).await?);

      let _ = self.state.connections.replace(
        key,
        ConnectionEntry {
          connection: entry.connection.clone(),
          metadata: new_metadata,
        },
        true,
      );
    }

    return Ok(());
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
  runtimes: Vec<(SqliteStore, SqliteFunctions)>,
) -> Result<(Connection, ConnectionMetadata, bool), ConnectionError> {
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
  runtimes: Vec<(SqliteStore, SqliteFunctions)>,
  main_migrations: bool,
) -> Result<(Connection, ConnectionMetadata, bool), ConnectionError> {
  let main_path = data_dir.map(|d| d.main_db_path());
  let migrations_path = data_dir.map(|d| d.migrations_path());

  for AttachedDatabase { schema_name, path } in &attach {
    debug!("Attaching '{schema_name}': {path:?}, {migrations_path:?}");

    if let Some(ref migrations_path) = migrations_path {
      // NOTE: that migrations may also depend on extension functions.
      // FIXME: Right now this will fail if user migrations depend on custom WASM SQLite functions.
      let mut secondary =
        trailbase_extension::connect_sqlite(Some(path.clone()), json_registry.clone())?;

      // The default is just 16.
      secondary.set_prepared_statement_cache_capacity(PREPARED_STATEMENT_CACHE_CAPACITY);

      #[cfg(any(feature = "geos", feature = "geos-static"))]
      litegis::register(&secondary)?;

      apply_base_migrations(&mut secondary, Some(migrations_path), &schema_name)?;
    }
  }

  let metadata: Arc<Mutex<Option<ConnectionMetadata>>> = Arc::new(Mutex::new(None));
  let mut new_db = AtomicBool::new(false);

  let conn = {
    let metadata = metadata.clone();
    let new_db = &mut new_db;

    let conn_builder = move || -> Result<_, trailbase_sqlite::Error> {
      let mut conn = trailbase_extension::connect_sqlite(main_path.clone(), json_registry.clone())
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

      // The default is just 16.
      conn.set_prepared_statement_cache_capacity(PREPARED_STATEMENT_CACHE_CAPACITY);

      #[cfg(any(feature = "geos", feature = "geos-static"))]
      litegis::register(&conn)?;

      if main_migrations {
        new_db.fetch_or(
          apply_main_migrations(&mut conn, migrations_path.as_ref())
            .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?,
          Ordering::SeqCst,
        );
      }

      for AttachedDatabase { schema_name, path } in &attach {
        conn.execute(
          &format!(
            "ATTACH DATABASE '{path}' AS {schema_name} ",
            path = path.to_string_lossy()
          ),
          (),
        )?;
      }

      // Install SQLite extension methods/functions registered by WASM components.
      #[cfg(feature = "wasm")]
      for (store, functions) in &runtimes {
        trailbase_wasm_runtime_host::functions::setup_connection(&conn, store.clone(), functions)
          .expect("startup");
      }

      // Build connection metadata
      {
        let mut lock = metadata.lock();
        if lock.is_none() {
          let json_registry = json_registry.as_ref().map_or_else(
            || Arc::new(RwLock::new(JsonSchemaRegistry::default())),
            |r| r.clone(),
          );

          let _ = lock.insert(
            build_metadata_sync(&conn, &json_registry)
              .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?,
          );
        }
      }

      return Ok(conn);
    };

    trailbase_sqlite::Connection::with_opts(
      conn_builder,
      trailbase_sqlite::Options {
        num_threads: match (data_dir, std::thread::available_parallelism()) {
          (None, _) => Some(1),
          (Some(_), Ok(n)) => Some(n.get().clamp(2, 4)),
          (Some(_), Err(_)) => Some(2),
        },
        ..Default::default()
      },
    )?
  };

  // NOTE: We could consider larger memory maps and caches for the main database.
  // Should be driven by benchmarks.
  // conn.pragma_update(None, "mmap_size", 268435456)?;
  // conn.pragma_update(None, "cache_size", -32768)?; // 32MB

  return Ok((
    conn,
    metadata
      .lock()
      .take()
      .ok_or_else(|| ConnectionError::MissingMetadata)?,
    new_db.load(Ordering::SeqCst),
  ));
}

pub(super) fn init_logs_db(
  data_dir: Option<&DataDir>,
) -> Result<Connection, trailbase_sqlite::Error> {
  let path = data_dir.map(|d| d.logs_db_path());

  return trailbase_sqlite::Connection::with_opts(
    || -> Result<_, trailbase_sqlite::Error> {
      // NOTE: The logs db needs the trailbase extensions for the maxminddb geoip lookup.
      let mut conn = connect_rusqlite_without_default_extensions_and_schemas(path.clone())?;

      trailbase_extension::register_all_extension_functions(&conn, None)?;

      // Turn off secure_deletions, i.e. don't wipe the memory with zeros.
      conn.pragma_update(None, "secure_delete", "FALSE")?;

      apply_logs_migrations(&mut conn).map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;
      return Ok(conn);
    },
    trailbase_sqlite::Options {
      // Only using the writer, no readers (except for admin dash).
      num_threads: Some(1),
      ..Default::default()
    },
  );
}

pub fn init_session_db(data_dir: Option<&DataDir>) -> Result<Connection, trailbase_sqlite::Error> {
  let path = data_dir.map(|d| d.session_db_path());

  return trailbase_sqlite::Connection::new(|| -> Result<_, trailbase_sqlite::Error> {
    // NOTE: The logs db needs the trailbase extensions for the maxminddb geoip lookup.
    let mut conn = connect_rusqlite_without_default_extensions_and_schemas(path.clone())?;

    trailbase_extension::register_all_extension_functions(&conn, None)?;

    apply_session_migrations(&mut conn)
      .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;
    return Ok(conn);
  });
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

  // The default is just 16.
  conn.set_prepared_statement_cache_capacity(PREPARED_STATEMENT_CACHE_CAPACITY);

  // Rusqlite's default is 5s.
  conn.busy_timeout(std::time::Duration::from_millis(5000))?;

  return Ok(conn);
}

const PREPARED_STATEMENT_CACHE_CAPACITY: usize = 256;
