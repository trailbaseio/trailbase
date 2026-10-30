use arc_swap::ArcSwap;
use quick_cache::sync::GuardResult;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use trailbase_extension::jsonschema::JsonSchemaRegistry;
use trailbase_schema::db::metadata::ConnectionMetadata;

pub use trailbase_sqlite::Connection;

use crate::data_dir::DataDir;
use crate::migrations::{
  apply_base_migrations, apply_logs_migrations, apply_main_migrations, apply_session_migrations,
};
use crate::schema_metadata::build_metadata_and_maybe_file_deletions;
use crate::wasm::{SqliteFunction, SqliteStore};

#[derive(Debug, Error)]
pub enum ConnectionError {
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

#[derive(Debug)]
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
  pub connection: Connection,
  pub metadata: Arc<ConnectionMetadata>,
}

struct ConnectionManagerState {
  // Properties retained for initializing new connections.
  data_dir: DataDir,
  json_schema_registry: Arc<trailbase_schema::json_schema::registry::JsonSchemaRegistry>,
  sqlite_function_runtimes: Vec<(SqliteStore, Vec<SqliteFunction>)>,
  read_only: bool,

  // Properties for caching connections:
  main: ArcSwap<ConnectionEntry>,
  connections: quick_cache::sync::Cache<ConnectionKey, ConnectionEntry>,

  #[allow(unused)]
  pg_uri: Option<String>,
}

impl Drop for ConnectionManagerState {
  fn drop(&mut self) {
    log::debug!("Dropped ConnectionManagerState");
  }
}

// A manager for multi-DB SQLite connections.
#[derive(Clone)]
pub struct ConnectionManager {
  state: Arc<ConnectionManagerState>,
}

pub struct Options {
  pub data_dir: DataDir,
  pub json_schema_registry: Arc<trailbase_schema::json_schema::registry::JsonSchemaRegistry>,
  pub sqlite_function_runtimes: Vec<(SqliteStore, Vec<SqliteFunction>)>,
  pub pg_uri: Option<String>,
  pub read_only: Option<bool>,
}

#[derive(Debug, Default)]
struct BuildOptions {
  attach_main: bool,
  attached_databases: Option<BTreeSet<String>>,
  num_threads: Option<usize>,
}

impl ConnectionManager {
  pub(crate) async fn new(opts: Options) -> Result<(Self, bool), ConnectionError> {
    let Options {
      data_dir,
      json_schema_registry,
      sqlite_function_runtimes,
      pg_uri,
      read_only,
    } = opts;

    let (main_conn, main_metadata, new_db) = init_db(
      if let Some(ref pg_uri) = pg_uri {
        DbUri::Pg(pg_uri.clone())
      } else {
        DbUri::Sqlite(Some(data_dir.main_db_path()))
      },
      InitDbOptions {
        migrations: Some(Migrations {
          main: true,
          path: &data_dir.migrations_path(),
        }),
        json_registry: &json_schema_registry,
        runtimes: &sqlite_function_runtimes,
        attach: vec![],
        num_threads: None,
        read_only,
      },
    )
    .await?;

    return Ok((
      Self {
        state: Arc::new(ConnectionManagerState {
          data_dir,
          json_schema_registry,
          sqlite_function_runtimes,
          read_only: read_only.unwrap_or(false),
          main: ArcSwap::from_pointee(ConnectionEntry {
            connection: main_conn,
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
  pub async fn new_for_test(
    data_dir: DataDir,
    json_schema_registry: Arc<trailbase_schema::json_schema::registry::JsonSchemaRegistry>,
    sqlite_function_runtimes: Vec<(SqliteStore, Vec<SqliteFunction>)>,
    pg_uri: Option<String>,
  ) -> Self {
    const READ_ONLY: bool = false;
    let (main_conn, main_metadata, new_db) = init_db(
      cfg_select! {
        feature = "pg-test" => DbUri::Pg(pg_uri.as_ref().expect("test").clone()),
        _ => DbUri::Sqlite(Some(data_dir.main_db_path())),
      },
      InitDbOptions {
        migrations: Some(Migrations {
          main: true,
          path: &data_dir.migrations_path(),
        }),
        json_registry: &json_schema_registry,
        runtimes: &sqlite_function_runtimes,
        attach: vec![],
        num_threads: None,
        read_only: Some(READ_ONLY),
      },
    )
    .await
    .unwrap();

    if !new_db {
      panic!("Expected 'fresh' DB for test");
    }

    return Self {
      state: Arc::new(ConnectionManagerState {
        data_dir,
        json_schema_registry,
        sqlite_function_runtimes,
        read_only: READ_ONLY,
        main: ArcSwap::from_pointee(ConnectionEntry {
          connection: main_conn,
          metadata: Arc::new(main_metadata),
        }),
        connections: quick_cache::sync::Cache::new(256),
        pg_uri,
      }),
    };
  }

  pub fn main_entry(&self) -> ConnectionEntry {
    return (**self.state.main.load()).clone();
  }

  pub async fn get_entry(
    &self,
    attach_main: bool,
    attached_databases: Option<BTreeSet<String>>,
  ) -> Result<ConnectionEntry, ConnectionError> {
    if attach_main && attached_databases.is_none() {
      return Ok(self.main_entry());
    }

    let key = ConnectionKey {
      main: attach_main,
      attached_databases: attached_databases.clone().unwrap_or_default(),
    };

    return match self.state.connections.get_value_or_guard(&key, None) {
      GuardResult::Value(entry) => Ok(entry.clone()),
      GuardResult::Guard(placeholder) => {
        let entry = self
          .build(BuildOptions {
            attach_main,
            attached_databases,
            num_threads: None,
          })
          .await?;

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
    name: &trailbase_schema::db::QualifiedName,
  ) -> Result<ConnectionEntry, ConnectionError> {
    return match name.database_schema.as_deref() {
      None => Ok(self.main_entry().clone()),
      Some("main") if self.state.pg_uri.is_none() => Ok(self.main_entry().clone()),
      Some("public") if self.state.pg_uri.is_some() => Ok(self.main_entry().clone()),
      Some(db) => {
        if matches!(db, "session" | "logs") {
          return Err(ConnectionError::InvalidSetting("Private db"));
        }

        self
          .get_entry(/* attach_main= */ false, Some([db.to_string()].into()))
          .await
      }
    };
  }

  /// Initialize an independent connection, to avoid any sort of tomfoolery like dropping attached
  /// databases. This is relatively expensive, thus limit the number of spawned threads to 1.
  pub(crate) async fn build_for_admin_ui_query_editor(
    &self,
    attached_databases: Option<BTreeSet<String>>,
  ) -> Result<ConnectionEntry, ConnectionError> {
    return self
      .build(BuildOptions {
        attach_main: true,
        attached_databases,
        num_threads: Some(1),
      })
      .await;
  }

  pub(crate) async fn build_for_tenant(
    &self,
    tenant_template: &str,
    tenant_id: uuid::Uuid,
  ) -> Result<ConnectionEntry, ConnectionError> {
    if self.state.pg_uri.is_some() {
      return Err(ConnectionError::InvalidSetting("PG not supported"));
    }

    let db_path = self
      .state
      .data_dir
      .data_path()
      .join(tenant_template)
      .join(format!("{tenant_id}.db"));

    let (conn, metadata, _new_db) = init_db_sqlite(
      Some(&db_path),
      InitDbOptions {
        migrations: Some(Migrations {
          main: false,
          path: &self.state.data_dir.migrations_path(),
        }),
        json_registry: &self.state.json_schema_registry,
        runtimes: &self.state.sqlite_function_runtimes,
        attach: vec![],
        num_threads: Some(1),
        read_only: Some(self.state.read_only),
      },
    )
    .await?;

    return Ok(ConnectionEntry {
      connection: conn,
      metadata: Arc::new(metadata),
    });
  }

  /// Builds new connections, does not cache. Caching is handled by `get_*` methods above.
  async fn build(&self, opts: BuildOptions) -> Result<ConnectionEntry, ConnectionError> {
    log::debug!("ConnectionManager::build({opts:?})");

    #[cfg(all(test, not(feature = "pg-test")))]
    if opts.attach_main && opts.attached_databases.is_none() {
      // Some code doesn't use cached connections to avoid side-effects (e.g. admin query handler).
      // However, this doesn't work specifically for SQLite "in-memory" connections, thus we need
      // this test-only workaround.
      return Ok(self.main_entry());
    }

    let attach = if let Some(attached_databases) = opts.attached_databases {
      // SQLite supports only up to 125 DBs per connection: https://sqlite.org/limits.html.
      if attached_databases.len() > 124 {
        return Err(ConnectionError::InvalidSetting("Too many databases"));
      }

      attached_databases
        .into_iter()
        .map(|name| {
          return match name.as_str() {
            "main" | "public" | "session" | "logs" => {
              Err(ConnectionError::InvalidSetting("Private db"))
            }
            name => Ok(AttachedDatabase::from_data_dir(&self.state.data_dir, name)),
          };
        })
        .collect::<Result<Vec<_>, _>>()?
    } else {
      vec![]
    };

    let (conn, metadata, _new_db) = init_db(
      if let Some(ref pg_uri) = self.state.pg_uri {
        assert!(
          opts.attach_main && attach.is_empty(),
          "Postgres does not (yet) support multi-DB: {attach:?}",
        );

        DbUri::Pg(pg_uri.clone())
      } else {
        DbUri::Sqlite(if opts.attach_main {
          Some(self.state.data_dir.main_db_path())
        } else {
          None
        })
      },
      InitDbOptions {
        migrations: Some(Migrations {
          main: opts.attach_main,
          path: &self.state.data_dir.migrations_path(),
        }),
        json_registry: &self.state.json_schema_registry,
        runtimes: &self.state.sqlite_function_runtimes,
        attach,
        num_threads: opts.num_threads,
        read_only: Some(self.state.read_only),
      },
    )
    .await?;

    return Ok(ConnectionEntry {
      connection: conn,
      metadata: Arc::new(metadata),
    });
  }

  // Updates connection metadata for cached connections.
  pub(crate) async fn rebuild_metadata(&self) -> Result<(), ConnectionError> {
    // Main
    {
      let new_metadata = Arc::new({
        let conn = self.state.main.load().connection.clone();
        build_metadata_and_maybe_file_deletions(
          &conn,
          self.state.json_schema_registry.clone(),
          /* setup_file_deletions= */ !self.state.read_only,
        )
        .await?
      });

      let connection = self.state.main.load().connection.clone();
      self.state.main.store(Arc::new(ConnectionEntry {
        connection,
        metadata: new_metadata,
      }))
    }

    // Others:
    for (key, entry) in self.state.connections.iter() {
      let new_metadata = Arc::new(
        build_metadata_and_maybe_file_deletions(
          &entry.connection,
          self.state.json_schema_registry.clone(),
          /* setup_file_deletions = */ !self.state.read_only,
        )
        .await?,
      );

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

  /// Closes all currently managed connections.
  ///
  /// Connections will cleanly shut down when their last reference is dropped. However, you can use
  /// this method to explicitly close all managed connections. This means that anyone else holding
  /// references to these shared connections, will have a safe but defunct connection.
  pub async fn close(&self) {
    for (_key, entry) in self.state.connections.drain() {
      if let Err(err) = entry.connection.close().await {
        log::warn!("Failed to close connection: {err}");
      }
    }

    if let Err(err) = self.main_entry().connection.close().await {
      log::warn!("Failed to close main connection: {err}");
    }
  }
}

/// The path of the DB to open or URI for Postgres. If none, "main" will be an in-memory SQLite DB.
enum DbUri {
  Sqlite(Option<PathBuf>),
  Pg(String),
}

#[derive(Debug)]
struct Migrations<'a> {
  /// Whether to apply main migrations (`_user` table, etc), or just based (custom + file deletions).
  main: bool,
  /// Path from where to load migrations: <migration_path>/<attach.schema_name>/*.sql.
  path: &'a Path,
}

struct InitDbOptions<'a> {
  /// List of additional, attached DBs.
  attach: Vec<AttachedDatabase>,
  /// Migration options.
  migrations: Option<Migrations<'a>>,
  /// JSON Schema registry to use for building metadata (not supported for Postgres).
  json_registry: &'a Arc<JsonSchemaRegistry>,
  /// WASM runtimes for provided SQLite extension functions.
  runtimes: &'a Vec<(SqliteStore, Vec<SqliteFunction>)>,
  /// Size of executor pool.
  num_threads: Option<usize>,
  /// Prevent writes to all open DBs (main + attached), only SQLite.
  read_only: Option<bool>,
}

async fn init_db<'a>(
  uri: DbUri,
  opts: InitDbOptions<'a>,
) -> Result<(Connection, ConnectionMetadata, bool), ConnectionError> {
  return match uri {
    DbUri::Sqlite(path) => init_db_sqlite(path.as_deref(), opts).await,
    DbUri::Pg(uri) => cfg_select! {
      feature = "pg" => init_db_pg(uri, opts).await,
      _ => {
        panic!("`pg_uri` provided, however server was built w/o pg support: {uri}")
      }
    },
  };
}

#[cfg(feature = "pg")]
async fn init_db_pg<'a>(
  pg_uri: String,
  opts: InitDbOptions<'a>,
) -> Result<(Connection, ConnectionMetadata, bool), ConnectionError> {
  let InitDbOptions {
    migrations,
    json_registry,
    runtimes,
    num_threads,
    ..
  } = opts;

  #[cfg(not(test))]
  log::warn!("Postgres support is experimental: {pg_uri:?}");

  if !runtimes.is_empty() {
    log::warn!(
      "WASM components provided SQLite extension functions, which cannot be supported by PG"
    );
  }

  let conn = trailbase_sqlite::Connection::pg_with_opts(trailbase_sqlite::generic::PgOptions {
    connection: trailbase_sqlite::generic::PgConnection::Uri(pg_uri),
    num_threads,
  })?;

  // Apply migrations.
  //
  // IMPORTANT: All extensions need to be loaded before to satisfy potential dependencies.
  let init_schema = if let Some(migrations) = migrations
    && migrations.main
  {
    crate::migrations::apply_pg_main_migrations(&conn, migrations.path)
      .await
      .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?
  } else {
    false
  };

  // NOTE: read_only not supported for PG.
  let metadata = build_metadata_and_maybe_file_deletions(
    &conn,
    json_registry.clone(),
    /* setup_file_deletions= */ true, // PG does not support RO.
  )
  .await?;

  return Ok((conn, metadata, init_schema));
}

async fn init_db_sqlite<'a>(
  data_path: Option<&'a Path>,
  opts: InitDbOptions<'a>,
) -> Result<(Connection, ConnectionMetadata, bool), ConnectionError> {
  let InitDbOptions {
    attach,
    migrations,
    json_registry,
    runtimes,
    num_threads,
    read_only,
  } = opts;

  // NOTE: Disable on debug builds and tests, just due to cargo handles workspace dependencies,
  // i.e. the optional pg-schema dependency pulls in `trailbase-sqlite` with the `generic`
  // feature enabled.
  #[cfg(all(not(debug_assertions), not(feature = "pg")))]
  fn _implicitly_assert_db_sqlite_connection_type() {
    // Assert that we're not using th the polymorphic connection in production sqlite mode. The
    // below constructor only exists on the sqlite flavor.
    trailbase_sqlite::Connection::new(|| -> Result<_, trailbase_sqlite::Error> {
      return Ok(rusqlite::Connection::open_in_memory()?);
    })
    .unwrap();
  }

  if attach.len() > 124 {
    return Err(ConnectionError::InvalidSetting("Too many databases"));
  }

  let conn = trailbase_sqlite::Connection::with_opts(
    {
      let (json_registry, runtimes) = (json_registry.clone(), runtimes.clone());

      move || -> Result<rusqlite::Connection, ConnectionError> {
        return build_rusqlite_connection(data_path, json_registry.clone(), &runtimes);
      }
    },
    trailbase_sqlite::Options {
      read_only,
      num_threads: Some(num_threads.unwrap_or_else(|| {
        // Fallback if not explicitly set.
        match (data_path, std::thread::available_parallelism()) {
          (None, _) => 1,
          (Some(_), Ok(n)) => n.get().clamp(2, 4),
          (Some(_), Err(_)) => 2,
        }
      })),
      ..Default::default()
    },
  )?;

  // Apply migrations.
  //
  // IMPORTANT: All extensions need to be loaded before to satisfy potential dependencies.
  let init_schema = match &migrations {
    Some(migrations) if migrations.main => apply_main_migrations(&conn, Some(migrations.path))
      .await
      .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?,
    Some(migrations) => {
      // TODO: We should apply base migrations. Right now blocked on migrations needing
      // rusqlite::Connection rather than SyncConnectionTrait.
      false
    }
    None => false,
  };

  for AttachedDatabase { schema_name, path } in &attach {
    log::debug!("Attaching '{schema_name}': {path:?}, {migrations:?}");

    // Before attaching secondary DBs, we must ensure their schemas are up-to-date.
    if let Some(ref migrations) = migrations {
      let mut secondary =
        build_rusqlite_connection(Some(path), opts.json_registry.clone(), opts.runtimes)?;

      // Apply migrations.
      //
      // IMPORTANT: All extensions need to be loaded before to satisfy potential dependencies.
      apply_base_migrations(&mut secondary, Some(migrations.path), schema_name)?;
    }

    conn.attach(&path.to_string_lossy(), schema_name).await?;
  }

  // Lastly, after attaching all DBs, build connection metadata.
  let metadata = build_metadata_and_maybe_file_deletions(
    &conn,
    json_registry.clone(),
    /* setup_file_deletions= */ !opts.read_only.unwrap_or(false),
  )
  .await?;

  return Ok((conn, metadata, init_schema));
}

fn build_rusqlite_connection(
  db_path: Option<&Path>,
  json_registry: Arc<JsonSchemaRegistry>,
  #[allow(unused)] runtimes: &[(SqliteStore, Vec<SqliteFunction>)],
) -> Result<rusqlite::Connection, ConnectionError> {
  log::trace!("building SQLite connection: {db_path:?}");

  let conn = trailbase_extension::connect_rusqlite(db_path, Some(json_registry))
    .map_err(trailbase_sqlite::Error::Rusqlite)?;

  // Apply custom connection settings, e.g. PRAGMAs and client settings.
  {
    // NOTE: We could consider larger memory maps and caches for the main database.
    // Should be driven by benchmarks.
    // conn.pragma_update(None, "mmap_size", 268435456)?;
    // conn.pragma_update(None, "cache_size", -32768)?; // 32MB
  }

  #[cfg(any(feature = "geos", feature = "geos-static"))]
  litegis::register(&conn).map_err(trailbase_sqlite::Error::Rusqlite)?;

  // Install SQLite extension methods/functions registered by WASM components.
  #[cfg(feature = "wasm")]
  for (store, functions) in runtimes {
    trailbase_wasm_runtime_host::sqlite_function_runtime::setup_connection(
      &conn,
      store.clone(),
      functions,
    )
    .map_err(trailbase_sqlite::Error::Rusqlite)?;
  }

  return Ok(conn);
}

pub(super) fn init_logs_db(
  data_dir: Option<&DataDir>,
) -> Result<Connection, trailbase_sqlite::Error> {
  let path = data_dir.map(|d| d.logs_db_path());

  return trailbase_sqlite::Connection::with_opts(
    || -> Result<_, trailbase_sqlite::Error> {
      // NOTE: The logs db needs the trailbase extensions for the maxminddb geoip lookup.
      let mut conn = trailbase_extension::connect_rusqlite(path.as_deref(), None)?;

      // On disk, deletions aren't actively overridden with zeros but merely marked for reuse. This
      // has minor performance advantages at the expense that someone with disk access may recover
      // stale data. That's fine for append-only logs.
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

pub fn init_session_db(
  data_dir: Option<&DataDir>,
  read_only: bool,
) -> Result<Connection, trailbase_sqlite::Error> {
  let path = data_dir.map(|d| d.session_db_path());

  return trailbase_sqlite::Connection::with_opts(
    || -> Result<_, trailbase_sqlite::Error> {
      let mut conn = trailbase_extension::connect_rusqlite_without_extensions(path.as_deref())?;

      apply_session_migrations(&mut conn)
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

      return Ok(conn);
    },
    trailbase_sqlite::Options {
      read_only: Some(read_only),
      ..Default::default()
    },
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn sqlite_cleans_up_auxiliary_files_on_close() {
    let tmp_dir = temp_dir::TempDir::new().unwrap();
    let data_dir = DataDir(tmp_dir.path().to_path_buf());
    data_dir.ensure_directory_structure().await.unwrap();

    let json_schema_registry = Arc::new(
      trailbase_schema::json_schema::registry::build_json_schema_registry(vec![]).unwrap(),
    );

    let (conn, _metadata, new_db) = init_db_sqlite(
      Some(&data_dir.main_db_path()),
      InitDbOptions {
        migrations: Some(Migrations {
          main: true,
          path: &data_dir.migrations_path().as_path(),
        }),
        json_registry: &json_schema_registry,
        runtimes: &vec![],
        attach: vec![AttachedDatabase {
          schema_name: "foo".to_string(),
          path: data_dir.data_path().join("foo.db"),
        }],
        num_threads: None,
        read_only: None,
      },
    )
    .await
    .unwrap();

    assert!(new_db);

    conn
      .execute(
        "CREATE TABLE 'test' (id INTEGER PRIMARY KEY, value INTEGER)",
        (),
      )
      .await
      .unwrap();

    const N: usize = 100;
    for i in 0..N {
      conn
        .execute("INSERT INTO 'test' (value) VALUES ($1);", (i as i64,))
        .await
        .unwrap();
    }

    assert_eq!(
      N,
      conn
        .read_query_rows("SELECT * FROM 'test'", ())
        .await
        .unwrap()
        .len()
    );

    assert!(conn.close().await.is_ok());

    // Make sure there are no auxiliary files left (i.e. -wal, -shm).
    let contents: Vec<_> = std::fs::read_dir(data_dir.data_path())
      .unwrap()
      .into_iter()
      .map(|entry| entry.unwrap().path())
      .collect();

    assert_eq!(2, contents.len(), "{contents:?}");
  }
}
