use itertools::Itertools;
use log::*;
use parking_lot::Mutex;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use trailbase_refinery::Migration;
use walkdir::{DirEntry, WalkDir};

const MIGRATION_TABLE_NAME: &str = "_schema_history";

pub fn new_unique_migration_filename(suffix: &str) -> String {
  let timestamp = {
    // We use the timestamp as a version. We need to debounce it to avoid collisions.
    static PREV_TIMESTAMP: LazyLock<Mutex<i64>> = LazyLock::new(|| Mutex::new(0));

    let now = chrono::Utc::now().timestamp();
    let mut prev = PREV_TIMESTAMP.lock();

    if now > *prev {
      *prev = now;
      now
    } else {
      *prev += 1;
      *prev
    }
  };

  return format!("U{timestamp}__{suffix}.sql");
}

pub(crate) fn new_migration_runner(migrations: &[Migration]) -> trailbase_refinery::Runner {
  // NOTE: divergent migrations are migrations with the same version but a different name. That
  // said, `set_abort_divergent` is not a viable way for us to handle collisions (e.g. in tests),
  // since setting it to false, will prevent the migration from failing but divergent migrations
  // are quietly dropped on the floor and not applied. That's not ok.
  let mut runner = trailbase_refinery::Runner::new(migrations).set_abort_divergent(false);
  runner.set_migration_table_name(MIGRATION_TABLE_NAME);
  return runner;
}

/// Apply migrations: embedded and from `user_mgiations_path`.
///
/// Returns true, if V1 was applied, i.e. DB is initialized for the first time,
/// otherwise false.
pub(crate) fn apply_main_migrations(
  conn: &mut rusqlite::Connection,
  user_migrations_path: Option<PathBuf>,
) -> Result<bool, trailbase_refinery::Error> {
  let mut migrations = vec![load_embedded_migrations::<MainMigrations>()];
  if let Some(path) = user_migrations_path {
    // if let Ok(user_migrations) = load_sql_migrations(path.join("main"), false) {
    //   migrations.push(user_migrations);
    // }

    // Legacy: all *.sql files in migrations.
    migrations.push(load_sql_migrations(path, true)?);
  }
  return apply_migrations("main", conn, migrations);
}

pub(crate) fn apply_logs_migrations(
  logs_conn: &mut rusqlite::Connection,
) -> Result<(), trailbase_refinery::Error> {
  apply_migrations(
    "logs",
    logs_conn,
    vec![load_embedded_migrations::<LogsMigrations>()],
  )?;
  return Ok(());
}

pub(crate) fn apply_migrations(
  name: &str,
  conn: &mut rusqlite::Connection,
  migrations: Vec<Vec<Migration>>,
) -> Result<bool, trailbase_refinery::Error> {
  let migrations: Vec<Migration> = migrations.into_iter().flatten().sorted().collect();

  let runner = new_migration_runner(&migrations);
  let report = runner.run(conn).map_err(|err| {
    error!("Migration error for '{name}' DB: {err}");
    return err;
  })?;

  let applied_migrations = report.applied_migrations();
  log_migrations(name, applied_migrations);

  // If we applied migration v1 we can be sure this is a fresh database.
  let new_db = applied_migrations.iter().any(|m| m.version() == 1);

  return Ok(new_db);
}

fn log_migrations(db_name: &str, migrations: &[Migration]) {
  fn name(migration: &Migration) -> String {
    return format!(
      "{prefix}{version}__{name}",
      prefix = migration.prefix(),
      version = migration.version(),
      name = migration.name(),
    );
  }

  if !migrations.is_empty() {
    if !cfg!(test) {
      info!(
        "Successfully applied migrations for '{db_name}' DB: {names}",
        names = migrations
          .iter()
          .map(|m| format!("'{}'", name(m)))
          .join(", ")
      )
    }

    for migration in migrations {
      trace!(
        "Migration details for '{name}':\n{sql}",
        name = name(migration),
        sql = migration.sql().unwrap_or("<EMPTY>"),
      );
    }
  }
}

/// Loads SQL migrations from a path. This enables dynamic migration discovery, as opposed to
/// embedding. The resulting collection is ordered by version.
pub(crate) fn load_sql_migrations(
  location: impl AsRef<Path>,
  recursive: bool,
) -> Result<Vec<Migration>, trailbase_refinery::Error> {
  use trailbase_refinery::{Error, error::Kind};

  let mut migrations = find_migration_files(location, recursive)?
    .map(|path| -> Result<Migration, Error> {
      let sql = std::fs::read_to_string(path.as_path()).map_err(|e| {
        let path = path.to_owned();
        let kind = match e.kind() {
          std::io::ErrorKind::NotFound => Kind::InvalidMigrationPath(path, e),
          _ => Kind::InvalidMigrationFile(path, e),
        };

        Error::new(kind, None)
      })?;

      let filename = path
        .file_stem()
        .and_then(|file| file.to_os_string().into_string().ok())
        .ok_or_else(|| trailbase_refinery::Error::new(Kind::InvalidName, None))?;

      return Migration::unapplied(&filename, &sql);
    })
    .collect::<Result<Vec<Migration>, Error>>()?;

  migrations.sort();

  return Ok(migrations);
}

const STEM_RE: &str = r"^([U|V])(\d+(?:\.\d+)?)__(\w+)";
static SQL_FILE_RE: LazyLock<regex::Regex> =
  LazyLock::new(|| regex::Regex::new(&format!(r"{STEM_RE}\.sql$")).expect("const"));

/// find migrations on file system recursively across directories given a location and
/// [MigrationType]
fn find_migration_files(
  location: impl AsRef<Path>,
  recursive: bool,
) -> Result<impl Iterator<Item = PathBuf>, trailbase_refinery::Error> {
  use trailbase_refinery::error::Kind;

  let location: &Path = location.as_ref();
  let location = location.canonicalize().map_err(|err| {
    trailbase_refinery::Error::new(
      Kind::InvalidMigrationPath(location.to_path_buf(), err),
      None,
    )
  })?;

  // NOTE: Don't load recursively.
  let file_paths = WalkDir::new(location)
    .max_depth(if recursive {usize::MAX} else {1})
    .into_iter()
    .filter_map(Result::ok)
    .map(DirEntry::into_path)
    // filter by migration file regex
    .filter(|path|-> bool {
    return match path.file_name().and_then(OsStr::to_str) {
      Some(_) if path.is_dir() => false,
      Some(file_name) if SQL_FILE_RE.is_match(file_name) => true,
      Some(file_name) => {
        log::warn!(
          "File \"{file_name}\" does not adhere to the migration naming convention. Migrations must be named in the format [U|V]{{1}}__{{2}}.sql or [U|V]{{1}}__{{2}}.rs, where {{1}} represents the migration version and {{2}} the name."
        );
        false
      }
      None => false,
    };
  });

  Ok(file_paths)
}

fn load_embedded_migrations<T: rust_embed::RustEmbed>() -> Vec<Migration> {
  return T::iter()
    .map(|filename| {
      return Migration::unapplied(
        &filename,
        &String::from_utf8_lossy(&T::get(&filename).expect("startup").data),
      )
      .expect("startup");
    })
    .collect();
}

#[derive(Clone, rust_embed::RustEmbed)]
#[folder = "migrations/main"]
struct MainMigrations;

#[derive(Clone, rust_embed::RustEmbed)]
#[folder = "migrations/logs"]
struct LogsMigrations;
