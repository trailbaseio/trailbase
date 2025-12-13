use log::*;
use parking_lot::{Mutex, RwLock};
use quick_cache::sync::GuardResult;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use trailbase_extension::jsonschema::JsonSchemaRegistry;
use trailbase_schema::metadata::ConnectionMetadata;
use trailbase_schema::sqlite::{Table, View};

use crate::data_dir::DataDir;
use crate::migrations::{apply_base_migrations, apply_logs_migrations, apply_main_migrations};
use crate::wasm::SqliteFunctionRuntime;

pub use trailbase_sqlite::Connection;

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
  #[error("Json Schema: {0}")]
  JsonSchema(#[from] trailbase_schema::metadata::JsonSchemaError),
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
  data_dir: DataDir,
  json_schema_registry: Arc<RwLock<trailbase_schema::registry::JsonSchemaRegistry>>,
  sqlite_function_runtimes: Vec<SqliteFunctionRuntime>,

  // Cache of existing Sqlite connections:
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
    sqlite_function_runtimes: Vec<SqliteFunctionRuntime>,
  ) -> Result<(Self, bool), ConnectionError> {
    let (main_conn, new_db) = init_main_db_impl(
      Some(&data_dir),
      Some(json_schema_registry.clone()),
      vec![],
      sqlite_function_runtimes.clone(),
      true,
    )?;

    let main_metadata = build_metadata(&main_conn.write_lock(), &json_schema_registry)?;

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
    sqlite_function_runtimes: Vec<SqliteFunctionRuntime>,
  ) -> Self {
    let (main_conn, new_db) = init_main_db_impl(
      None,
      Some(json_schema_registry.clone()),
      vec![],
      sqlite_function_runtimes.clone(),
      true,
    )
    .unwrap();
    assert!(new_db);

    let main_metadata = build_metadata(&main_conn.write_lock(), &json_schema_registry).unwrap();

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

  // pub(crate) fn main(&self) -> Arc<Connection> {
  //   return self.state.main.read().connection.clone();
  // }

  pub fn main_entry(&self) -> ConnectionEntry {
    return self.state.main.read().clone();
  }

  // pub(crate) fn get(
  //   &self,
  //   main: bool,
  //   attached_databases: Option<BTreeSet<String>>,
  // ) -> Result<Arc<Connection>, ConnectionError> {
  //   return Ok(self.get_entry(main, attached_databases)?.connection);
  // }

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
        let conn = self.build(main, Some(&key.attached_databases))?;

        let entry = ConnectionEntry {
          connection: conn.clone(),
          metadata: Arc::new(build_metadata(
            &conn.write_lock(),
            &self.state.json_schema_registry,
          )?),
        };

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
      return self.get_entry(false, Some([db.to_string()].into()));
    }

    return Ok(self.main_entry());
  }

  pub(crate) fn build(
    &self,
    mut main: bool,
    attached_databases: Option<&BTreeSet<String>>,
  ) -> Result<Arc<Connection>, ConnectionError> {
    #[cfg(test)]
    if main && attached_databases.is_none() {
      return Ok(self.state.main.read().connection.clone());
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

    let (conn, _new_db) = init_main_db_impl(
      Some(&self.state.data_dir),
      Some(self.state.json_schema_registry.clone()),
      attach,
      self.state.sqlite_function_runtimes.clone(),
      main,
    )?;

    return Ok(Arc::new(conn));
  }

  // Updates connection metadata for cached connections.
  pub(crate) fn rebuild_metadata(&self) -> Result<(), ConnectionError> {
    // Main
    {
      let new_metadata = Arc::new(build_metadata(
        &self.state.main.read().connection.write_lock(),
        &self.state.json_schema_registry,
      )?);

      self.state.main.write().metadata = new_metadata;
    }

    // Others:
    for (key, entry) in self.state.connections.iter() {
      let new_metadata = Arc::new(build_metadata(
        &entry.connection.write_lock(),
        &self.state.json_schema_registry,
      )?);

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

pub(crate) fn build_metadata(
  conn: &rusqlite::Connection,
  json_schema_registry: &Arc<RwLock<trailbase_schema::registry::JsonSchemaRegistry>>,
) -> Result<ConnectionMetadata, ConnectionError> {
  use crate::schema_metadata::*;
  let tables = lookup_and_parse_all_table_schemas_sync(conn)?;
  let views = lookup_and_parse_all_view_schemas_sync(conn, &tables)?;

  return build_connection_metadata_and_install_file_deletion_triggers_sync(
    conn,
    tables,
    views,
    json_schema_registry,
  );
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
    let json_registry = json_registry.clone();

    trailbase_sqlite::Connection::new(
      move || -> Result<_, ConnectionError> {
        let mut conn =
          trailbase_extension::connect_sqlite(main_path.clone(), json_registry.clone())?;

        if main_migrations {
          *(new_db.lock()) |= apply_main_migrations(&mut conn, migrations_path.as_ref())?;
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
    debug!("Attaching '{schema_name}': {path:?}, {migrations_path:?}");

    if let Some(ref migrations_path) = migrations_path {
      // NOTE: that migrations may also depend on extension functions.
      // FIXME: Right now this will fail if user migrations depend on custom WASM SQLite functions.
      let mut secondary =
        trailbase_extension::connect_sqlite(Some(path.clone()), json_registry.clone())?;

      apply_base_migrations(&mut secondary, Some(migrations_path), &schema_name)?;
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

  // Rusqlite's default is 5s.
  conn.busy_timeout(std::time::Duration::from_millis(5000))?;

  return Ok(conn);
}

/// (Re-)build the connections schema representation *with* the side-effect of (re-)installing file
/// deletion triggers.
///
/// Tying the construction of schema metadata and the (re-)installing of file deletion triggers so
/// closely together is a necessary evil. For example, whenever a schema changes, e.g. a new file
/// column is added, we need to rebuild the metadata and update or install missing triggers.
fn build_connection_metadata_and_install_file_deletion_triggers_sync(
  conn: &rusqlite::Connection,
  tables: Vec<Table>,
  views: Vec<View>,
  registry: &RwLock<JsonSchemaRegistry>,
) -> Result<ConnectionMetadata, ConnectionError> {
  let metadata = ConnectionMetadata::from_schemas(tables, views, &registry.read())?;

  setup_file_deletion_triggers_sync(conn, &metadata)?;

  return Ok(metadata);
}

// Install file column triggers. This ain't pretty, this might be better on construction and
// schema changes.
fn setup_file_deletion_triggers_sync(
  conn: &rusqlite::Connection,
  metadata: &ConnectionMetadata,
) -> Result<(), trailbase_sqlite::Error> {
  for metadata in metadata.tables.values() {
    for idx in metadata.json_metadata.file_column_indexes() {
      let table_name = &metadata.schema.name;
      let unqualified_name = &metadata.schema.name.name;
      let db = metadata
        .schema
        .name
        .database_schema
        .as_deref()
        .unwrap_or("main");

      let col = &metadata.schema.columns[*idx];
      let column_name = &col.name;

      conn.execute_batch(&indoc::formatdoc!(
          r#"
          DROP TRIGGER IF EXISTS "{db}"."__{unqualified_name}__{column_name}__update_trigger";
          CREATE TRIGGER IF NOT EXISTS "{db}"."__{unqualified_name}__{column_name}__update_trigger" AFTER UPDATE ON {table_name}
            WHEN OLD."{column_name}" IS NOT NULL AND OLD."{column_name}" != NEW."{column_name}"
            BEGIN
              INSERT INTO _file_deletions (table_name, record_rowid, column_name, json) VALUES
                ('{table_name}', OLD._rowid_, '{column_name}', OLD."{column_name}");
            END;

          DROP TRIGGER IF EXISTS "{db}"."__{unqualified_name}__{column_name}__delete_trigger";
          CREATE TRIGGER IF NOT EXISTS "{db}"."__{unqualified_name}__{column_name}__delete_trigger" AFTER DELETE ON {table_name}
            WHEN OLD."{column_name}" IS NOT NULL
            BEGIN
              INSERT INTO _file_deletions (table_name, record_rowid, column_name, json) VALUES
                ('{table_name}', OLD._rowid_, '{column_name}', OLD."{column_name}");
            END;
          "#,
          table_name = table_name.escaped_string(),
        ))?;
    }
  }

  return Ok(());
}
