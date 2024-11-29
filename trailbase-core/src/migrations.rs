use lazy_static::lazy_static;
use log::*;
use parking_lot::Mutex;
use refinery::Migration;
use std::path::PathBuf;

mod main {
  refinery::embed_migrations!("migrations/main");
}
mod logs {
  refinery::embed_migrations!("migrations/logs");
}

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

pub(crate) fn new_migration_runner(migrations: &[Migration]) -> refinery::Runner {
  // NOTE: divergent migrations are migrations with the same version but a different name. That
  // said, `set_abort_divergent` is not a viable way for us to handle collisions (e.g. in tests),
  // since setting it to false, will prevent the migration from failing but divergent migrations
  // are quietly dropped on the floor and not applied. That's not ok.
  let mut runner = refinery::Runner::new(migrations).set_abort_divergent(false);
  runner.set_migration_table_name(MIGRATION_TABLE_NAME);
  return runner;
}

pub(crate) fn apply_main_migrations(
  conn: &mut rusqlite::Connection,
  user_migrations_path: Option<PathBuf>,
) -> Result<bool, refinery::Error> {
  let all_migrations = {
    let mut migrations: Vec<Migration> = vec![];

    let system_migrations_runner = main::migrations::runner();
    migrations.extend(system_migrations_runner.get_migrations().iter().cloned());

    if let Some(path) = user_migrations_path {
      // NOTE: refinery has a bug where it will name-check the directory and write a warning... :/.
      let user_migrations = refinery::load_sql_migrations(path)?;
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
      error!("Main migrations: {err}");
      return Err(err);
    }
  };

  for applied_migration in report.applied_migrations() {
    if cfg!(test) {
      debug!("applied migration: {applied_migration:?}");
    } else {
      info!("applied migration: {applied_migration:?}");
    }
  }

  // If we applied migration v1 we can be sure this is a fresh database.
  let new_db = report.applied_migrations().iter().any(|m| m.version() == 1);

  return Ok(new_db);
}

#[cfg(test)]
pub(crate) fn apply_user_migrations(
  user_conn: &mut rusqlite::Connection,
) -> Result<(), refinery::Error> {
  let mut runner = main::migrations::runner();
  runner.set_migration_table_name(MIGRATION_TABLE_NAME);

  let report = runner.run(user_conn).map_err(|err| {
    error!("User migrations: {err}");
    return err;
  })?;

  if cfg!(test) {
    debug!("user migrations: {report:?}");
  } else {
    info!("user migrations: {report:?}");
  }

  return Ok(());
}

pub(crate) fn apply_logs_migrations(
  logs_conn: &mut rusqlite::Connection,
) -> Result<(), refinery::Error> {
  let mut runner = logs::migrations::runner();
  runner.set_migration_table_name(MIGRATION_TABLE_NAME);

  let report = runner.run(logs_conn).map_err(|err| {
    error!("Logs migrations: {err}");
    return err;
  })?;

  if cfg!(test) {
    debug!("Logs migrations: {report:?}");
  } else {
    info!("Logs migrations: {report:?}");
  }

  return Ok(());
}
