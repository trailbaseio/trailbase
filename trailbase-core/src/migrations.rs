use itertools::Itertools;
use lazy_static::lazy_static;
use log::*;
use parking_lot::Mutex;
use std::path::PathBuf;
use trailbase_refinery::Migration;

const MIGRATION_TABLE_NAME: &str = "_schema_history";

pub fn new_unique_migration_filename(suffix: &str) -> String {
  let timestamp = {
    // We use the timestamp as a version. We need to debounce it to avoid collisions.
    lazy_static! {
      static ref PREV_TIMESTAMP: Mutex<i64> = Mutex::new(0);
    }

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

fn load_migrations<T: rust_embed::RustEmbed>() -> Vec<Migration> {
  let mut migrations = vec![];
  for filename in T::iter() {
    if let Some(file) = T::get(&filename) {
      migrations.push(
        Migration::unapplied(&filename, &String::from_utf8_lossy(&file.data)).expect("startup"),
      )
    }
  }
  return migrations;
}

pub(crate) fn apply_main_migrations(
  conn: &mut rusqlite::Connection,
  user_migrations_path: Option<PathBuf>,
) -> Result<bool, trailbase_refinery::Error> {
  let all_migrations = {
    let mut migrations: Vec<Migration> = vec![];

    let system_migrations_runner: Vec<Migration> = load_migrations::<MainMigrations>();
    migrations.extend(system_migrations_runner);

    if let Some(path) = user_migrations_path {
      // NOTE: refinery has a bug where it will name-check the directory and write a warning... :/.
      let user_migrations = trailbase_refinery::load_sql_migrations(path)?;
      migrations.extend(user_migrations);
    }

    // Interleave the system and user migrations based on their version prefixes.
    migrations.sort();

    migrations
  };

  let runner = new_migration_runner(&all_migrations);
  let report = match runner.run(conn) {
    Ok(report) => report,
    Err(err) => {
      error!("Migration error for 'main' DB: {err}");
      return Err(err);
    }
  };

  let applied_migrations = report.applied_migrations();
  log_migrations("main", applied_migrations);

  // If we applied migration v1 we can be sure this is a fresh database.
  let new_db = applied_migrations.iter().any(|m| m.version() == 1);

  return Ok(new_db);
}

pub(crate) fn apply_logs_migrations(
  logs_conn: &mut rusqlite::Connection,
) -> Result<(), trailbase_refinery::Error> {
  let migrations = load_migrations::<LogsMigrations>();

  let mut runner = new_migration_runner(&migrations);
  runner.set_migration_table_name(MIGRATION_TABLE_NAME);

  let report = runner.run(logs_conn).map_err(|err| {
    error!("Migration error for 'logs' DB: {err}");
    return err;
  })?;

  log_migrations("logs", report.applied_migrations());

  return Ok(());
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

#[derive(Clone, rust_embed::RustEmbed)]
#[folder = "migrations/main"]
struct MainMigrations;

#[derive(Clone, rust_embed::RustEmbed)]
#[folder = "migrations/logs"]
struct LogsMigrations;
