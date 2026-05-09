use parking_lot::RwLock;
use quick_cache::sync::GuardResult;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use trailbase_extension::jsonschema::JsonSchemaRegistry;
use trailbase_schema::metadata::ConnectionMetadata;

pub use trailbase_sqlite::{Connection, unpack_other_error};

use crate::data_dir::DataDir;
use crate::migrations::{
  apply_base_migrations, apply_logs_migrations, apply_main_migrations, apply_session_migrations,
};
use crate::schema_metadata::build_metadata;
use crate::wasm::{SqliteFunctions, SqliteStore};

#[derive(Debug, Error)]
pub enum ConnectionError {
  #[error("ExtensionError: {0}")]
  Extension(#[from] trailbase_extension::Error),
  #[error("SqlError: {0}")]
  Sql(#[from] trailbase_sqlite::Error),
  #[error("SchemaError: {0}")]
  Schema(#[from] crate::schema_metadata::SchemaLookupError),
  #[error("Migration: {0}")]
  Migration(#[from] trailbase_refinery::Error),
  #[error("MissingMetadata")]
  MissingMetadata,
  #[error("Timeout")]
  Timeout,
  #[error("SettingsError: {0}")]
  InvalidSetting(&'static str),
  // Used during runtime config/schema reloads.
  #[error("ConfigError: {0}")]
  ConfigError(#[from] crate::config::ConfigError),
}

// Packaging helper.
impl From<ConnectionError> for trailbase_sqlite::Error {
  fn from(err: ConnectionError) -> Self {
    return trailbase_sqlite::Error::Other(err.into());
  }
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
  pg_uri: Option<String>,
}

// A manager for multi-DB SQLite connections.
//
// NOTE: Performance-wise it's beneficial to share Connections to benefit from its internal locking
// instead of relying on SQLite's own file locking.
#[derive(Clone)]
pub struct ConnectionManager {
  state: Arc<ConnectionManagerState>,
}

pub struct Options {
  pub data_dir: DataDir,
  pub json_schema_registry: Arc<RwLock<trailbase_schema::registry::JsonSchemaRegistry>>,
  pub sqlite_function_runtimes: Vec<(SqliteStore, SqliteFunctions)>,
  pub pg_uri: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct BuildOptions {
  pub is_main: bool,
  pub attached_databases: Option<BTreeSet<String>>,
  pub num_threads: Option<usize>,
}

impl ConnectionManager {
  pub(crate) async fn new(opts: Options) -> Result<(Self, bool), ConnectionError> {
    let (main_conn, main_metadata, new_db) = init_db(InitDbOptions {
      data_path: Some(&opts.data_dir.main_db_path()),
      migration_path: Some(&opts.data_dir.migrations_path()),
      is_main_db: true,
      json_registry: &opts.json_schema_registry,
      runtimes: &opts.sqlite_function_runtimes,
      attach: vec![],
      num_threads: None,
      pg_uri: opts.pg_uri.clone(),
    })
    .await?;

    let Options {
      data_dir,
      json_schema_registry,
      sqlite_function_runtimes,
      pg_uri,
    } = opts;

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
          pg_uri,
        }),
      },
      new_db,
    ));
  }

  #[cfg(test)]
  pub(crate) async fn new_for_test(
    data_dir: DataDir,
    json_schema_registry: Arc<RwLock<trailbase_schema::registry::JsonSchemaRegistry>>,
    sqlite_function_runtimes: Vec<(SqliteStore, SqliteFunctions)>,
    pg_uri: Option<String>,
  ) -> Self {
    let (main_conn, main_metadata, new_db) = init_db(InitDbOptions {
      data_path: None,
      migration_path: None,
      is_main_db: true,
      json_registry: &json_schema_registry,
      runtimes: &sqlite_function_runtimes,
      attach: vec![],
      num_threads: None,
      pg_uri: pg_uri.clone(),
    })
    .await
    .unwrap();

    if !new_db {
      if cfg!(feature = "pg") {
        log::warn!("Re-using existing DB for test :/. Should only happen for shared external DBs.");
      } else {
        panic!("Expected 'fresh' DB for test");
      }
    }

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
        pg_uri,
      }),
    };
  }

  pub fn main_entry(&self) -> ConnectionEntry {
    return self.state.main.read().clone();
  }

  pub async fn get_entry(&self, opts: BuildOptions) -> Result<ConnectionEntry, ConnectionError> {
    if opts.is_main && opts.attached_databases.is_none() {
      return Ok(self.state.main.read().clone());
    }

    let key = ConnectionKey {
      main: opts.is_main,
      attached_databases: opts.attached_databases.clone().unwrap_or_default(),
    };

    return match self.state.connections.get_value_or_guard(&key, None) {
      GuardResult::Value(entry) => Ok(entry.clone()),
      GuardResult::Guard(placeholder) => {
        let entry = self.build(opts).await?;
        let _ = placeholder.insert(entry.clone());
        Ok(entry)
      }
      GuardResult::Timeout => {
        return Err(ConnectionError::Timeout);
      }
    };
  }

  pub async fn get_entry_for_qn(
    &self,
    name: &trailbase_schema::QualifiedName,
  ) -> Result<ConnectionEntry, ConnectionError> {
    if let Some(ref db) = name.database_schema
      && db != "main"
    {
      // QUESTION: Should we disallow access to "logs", "auth", etc? Currently, this is not
      // exposed to WASM, i.e. there's no sanctioned way to interact with this.
      return self
        .get_entry(BuildOptions {
          is_main: false,
          attached_databases: Some([db.to_string()].into()),
          ..Default::default()
        })
        .await;
    }

    return Ok(self.main_entry());
  }

  pub(crate) async fn build(&self, opts: BuildOptions) -> Result<ConnectionEntry, ConnectionError> {
    #[cfg(test)]
    if opts.is_main && opts.attached_databases.is_none() {
      return Ok(self.state.main.read().clone());
    }

    let mut is_main = opts.is_main;
    let attach = if let Some(attached_databases) = opts.attached_databases {
      // SQLite supports only up to 125 DBs per connection: https://sqlite.org/limits.html.
      if attached_databases.len() > 124 {
        return Err(ConnectionError::InvalidSetting("Too many databases"));
      }

      attached_databases
        .iter()
        .flat_map(|name| {
          if name != "main" {
            Some(AttachedDatabase::from_data_dir(&self.state.data_dir, name))
          } else {
            is_main = true;
            None
          }
        })
        .collect()
    } else {
      vec![]
    };

    let (conn, metadata, _new_db) = init_db(InitDbOptions {
      data_path: Some(&self.state.data_dir.main_db_path()),
      migration_path: Some(&self.state.data_dir.migrations_path()),
      is_main_db: is_main,
      json_registry: &self.state.json_schema_registry,
      runtimes: &self.state.sqlite_function_runtimes,
      attach,
      num_threads: opts.num_threads,
      pg_uri: self.state.pg_uri.clone(),
    })
    .await?;

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
        build_metadata(&conn, &self.state.json_schema_registry).await?
      });

      self.state.main.write().metadata = new_metadata;
    }

    // Others:
    for (key, entry) in self.state.connections.iter() {
      let new_metadata =
        Arc::new(build_metadata(&entry.connection, &self.state.json_schema_registry).await?);

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

struct InitDbOptions<'a> {
  data_path: Option<&'a PathBuf>,
  migration_path: Option<&'a PathBuf>,
  is_main_db: bool,
  json_registry: &'a Arc<RwLock<JsonSchemaRegistry>>,
  runtimes: &'a Vec<(SqliteStore, SqliteFunctions)>,
  attach: Vec<AttachedDatabase>,
  num_threads: Option<usize>,

  #[allow(unused)]
  pg_uri: Option<String>,
}

#[cfg(feature = "pg")]
async fn init_db<'a>(
  opts: InitDbOptions<'a>,
) -> Result<(Connection, ConnectionMetadata, bool), ConnectionError> {
  let conn = trailbase_sqlite::Connection::pg_with_opts(if let Some(uri) = opts.pg_uri {
    trailbase_sqlite::generic::PgOptions {
      connection: trailbase_sqlite::generic::PgConnection::Uri(uri),
      num_threads: Some(1),
    }
  } else {
    let host = trailbase_sqlite::generic::PgConnection::Host {
      host: Some("127.0.0.1".to_string()),
      port: Some(5432),
      user: Some("postgres".to_string()),
      password: Some("example".to_string()),
    };
    log::warn!("External Postgres required: {host:?}");

    trailbase_sqlite::generic::PgOptions {
      connection: host,
      num_threads: Some(2),
    }
  })?;

  // Apply migrations.
  //
  // IMPORTANT: All extensions need to be loaded before to satisfy potential dependencies.
  let init_schema = if opts.is_main_db {
    apply_main_migrations(&conn, opts.migration_path)
      .await
      .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?
  } else {
    false
  };

  // TODO: Remove sanity check.
  conn
    .read_query_rows(
      r#"SELECT EXISTS(SELECT 1 FROM "_user" WHERE email = 'foo')"#,
      (),
    )
    .await
    .unwrap();

  return Ok((conn, Default::default(), init_schema));
}

#[cfg(not(feature = "pg"))]
async fn init_db<'a>(
  opts: InitDbOptions<'a>,
) -> Result<(Connection, ConnectionMetadata, bool), ConnectionError> {
  if opts.attach.len() > 124 {
    return Err(ConnectionError::InvalidSetting("Too many databases"));
  }

  fn build_connection(
    db_path: Option<PathBuf>,
    json_registry: Arc<RwLock<JsonSchemaRegistry>>,
    runtimes: &Vec<(SqliteStore, SqliteFunctions)>,
  ) -> Result<rusqlite::Connection, ConnectionError> {
    let conn = trailbase_extension::connect_sqlite(db_path, Some(json_registry))?;

    // Apply custom connection settings, e.g. PRAGMAs and client settings.
    {
      // The default is just 16.
      conn.set_prepared_statement_cache_capacity(PREPARED_STATEMENT_CACHE_CAPACITY);

      // NOTE: We could consider larger memory maps and caches for the main database.
      // Should be driven by benchmarks.
      // conn.pragma_update(None, "mmap_size", 268435456)?;
      // conn.pragma_update(None, "cache_size", -32768)?; // 32MB
    }

    #[cfg(any(feature = "geos", feature = "geos-static"))]
    litegis::register(&conn).map_err(trailbase_extension::Error::Rusqlite)?;

    // Install SQLite extension methods/functions registered by WASM components.
    #[cfg(feature = "wasm")]
    for (store, functions) in runtimes {
      trailbase_wasm_runtime_host::functions::setup_connection(&conn, store.clone(), functions)
        .map_err(trailbase_extension::Error::Rusqlite)?;
    }

    return Ok(conn);
  }

  let conn = trailbase_sqlite::Connection::with_opts(
    {
      let data_path = opts.data_path.cloned();
      let json_registry = opts.json_registry.clone();
      let runtimes = opts.runtimes.clone();

      move || -> Result<rusqlite::Connection, ConnectionError> {
        return build_connection(data_path.clone(), json_registry.clone(), &runtimes);
      }
    },
    trailbase_sqlite::Options {
      num_threads: Some(opts.num_threads.unwrap_or_else(|| {
        // Fallback if not explicitly set.
        match (opts.data_path, std::thread::available_parallelism()) {
          (None, _) => 1,
          (Some(_), Ok(n)) => n.get().clamp(2, 4),
          (Some(_), Err(_)) => 2,
        }
      })),
      ..Default::default()
    },
  )
  .map_err(|err| {
    // Unpack potentially packed ConnectionError.
    return match unpack_other_error::<ConnectionError>(err) {
      Ok(schema_lookup_err) => schema_lookup_err,
      Err(sql_err) => sql_err.into(),
    };
  })?;

  // Apply migrations.
  //
  // IMPORTANT: All extensions need to be loaded before to satisfy potential dependencies.
  let init_schema = if opts.is_main_db {
    apply_main_migrations(&conn, opts.migration_path)
      .await
      .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?
  } else {
    false
  };

  for AttachedDatabase { schema_name, path } in &opts.attach {
    log::debug!(
      "Attaching '{schema_name}': {path:?}, {:?}",
      opts.migration_path
    );

    // Before attaching secondary DBs, we must ensure their schemas are up-to-date.
    if let Some(ref migrations_path) = opts.migration_path {
      let mut secondary = build_connection(
        Some(path.clone()),
        opts.json_registry.clone(),
        opts.runtimes,
      )?;

      // Apply migrations.
      //
      // IMPORTANT: All extensions need to be loaded before to satisfy potential dependencies.
      apply_base_migrations(&mut secondary, Some(migrations_path), schema_name)?;
    }

    conn.attach(&path.to_string_lossy(), schema_name).await?;
  }

  // Lastly, after attaching all DBs, build connection metadata.
  let metadata = build_metadata(&conn, opts.json_registry).await?;

  return Ok((conn, metadata, init_schema));
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

  return trailbase_sqlite::Connection::with_opts(
    || -> Result<_, trailbase_sqlite::Error> {
      // NOTE: The logs db needs the trailbase extensions for the maxminddb geoip lookup.
      let mut conn = connect_rusqlite_without_default_extensions_and_schemas(path.clone())?;

      trailbase_extension::register_all_extension_functions(&conn, None)?;

      apply_session_migrations(&mut conn)
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

      return Ok(conn);
    },
    Default::default(),
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

  // The default is just 16.
  conn.set_prepared_statement_cache_capacity(PREPARED_STATEMENT_CACHE_CAPACITY);

  // Rusqlite's default is 5s.
  conn.busy_timeout(std::time::Duration::from_millis(5000))?;

  return Ok(conn);
}

const PREPARED_STATEMENT_CACHE_CAPACITY: usize = 256;

#[cfg(test)]
mod tests {
  #[test]
  fn test_db_connetion_type() {
    // Assert that we're not using th the polymorphic connection in non-pg mode. The below
    // constructor only exists on sqlite flavor.

    #[cfg(not(feature = "pg"))]
    trailbase_sqlite::Connection::new(|| rusqlite::Connection::open_in_memory()).unwrap();
  }
}
