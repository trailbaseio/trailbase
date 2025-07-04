use crate::error::WrapMigrationError;
use crate::traits::{
  ASSERT_MIGRATIONS_TABLE_QUERY, GET_APPLIED_MIGRATIONS_QUERY, GET_LAST_APPLIED_MIGRATION_QUERY,
  insert_migration_query, verify_migrations,
};
use crate::{Error, Migration, Report, Target};

use async_trait::async_trait;
use std::string::ToString;

#[async_trait]
pub trait AsyncTransaction {
  type Error: std::error::Error + Send + Sync + 'static;

  async fn execute<'a, T: Iterator<Item = &'a str> + Send>(
    &mut self,
    queries: T,
  ) -> Result<usize, Self::Error>;
}

#[async_trait]
pub trait AsyncQuery<T>: AsyncTransaction {
  async fn query(&mut self, query: &str) -> Result<T, Self::Error>;
}

async fn migrate<T: AsyncTransaction>(
  transaction: &mut T,
  migrations: Vec<Migration>,
  target: Target,
  migration_table_name: &str,
) -> Result<Report, Error> {
  let mut applied_migrations = vec![];

  for mut migration in migrations.into_iter() {
    if let Target::Version(input_target) = target {
      if input_target < migration.version() {
        log::info!("stopping at migration: {input_target}, due to user option");
        break;
      }
    }

    log::info!("applying migration: {migration}");
    migration.set_applied();
    let update_query = insert_migration_query(&migration, migration_table_name);
    transaction
      .execute(
        [
          migration.sql().as_ref().expect("sql must be Some!"),
          update_query.as_str(),
        ]
        .into_iter(),
      )
      .await
      .migration_err(
        &format!("error applying migration {migration}"),
        Some(&applied_migrations),
      )?;
    applied_migrations.push(migration);
  }
  Ok(Report::new(applied_migrations))
}

async fn migrate_grouped<T: AsyncTransaction>(
  transaction: &mut T,
  migrations: Vec<Migration>,
  target: Target,
  migration_table_name: &str,
) -> Result<Report, Error> {
  let mut grouped_migrations = Vec::new();
  let mut applied_migrations = Vec::new();

  for mut migration in migrations.into_iter() {
    if let Target::Version(input_target) | Target::FakeVersion(input_target) = target {
      if input_target < migration.version() {
        break;
      }
    }

    migration.set_applied();
    let query = insert_migration_query(&migration, migration_table_name);

    let sql = migration.sql().expect("sql must be Some!").to_string();

    // If Target is Fake, we only update schema migrations table
    if !matches!(target, Target::Fake | Target::FakeVersion(_)) {
      applied_migrations.push(migration);
      grouped_migrations.push(sql);
    }
    grouped_migrations.push(query);
  }

  match target {
    Target::Fake | Target::FakeVersion(_) => {
      log::info!("not going to apply any migration as fake flag is enabled");
    }
    Target::Latest | Target::Version(_) => {
      log::info!(
        "going to apply batch migrations in single transaction: {:#?}",
        applied_migrations.iter().map(ToString::to_string)
      );
    }
  };

  if let Target::Version(input_target) = target {
    log::info!("stopping at migration: {input_target}, due to user option");
  }

  let refs = grouped_migrations.iter().map(AsRef::as_ref);

  transaction
    .execute(refs)
    .await
    .migration_err("error applying migrations", None)?;

  Ok(Report::new(applied_migrations))
}

#[async_trait]
pub trait AsyncMigrate: AsyncQuery<Vec<Migration>>
where
  Self: Sized,
{
  // Needed cause some database vendors like Mssql have a non sql standard way of checking the
  // migrations table
  fn assert_migrations_table_query(migration_table_name: &str) -> String {
    ASSERT_MIGRATIONS_TABLE_QUERY.replace("%MIGRATION_TABLE_NAME%", migration_table_name)
  }

  async fn get_last_applied_migration(
    &mut self,
    migration_table_name: &str,
  ) -> Result<Option<Migration>, Error> {
    let mut migrations = self
      .query(
        &GET_LAST_APPLIED_MIGRATION_QUERY.replace("%MIGRATION_TABLE_NAME%", migration_table_name),
      )
      .await
      .migration_err("error getting last applied migration", None)?;

    Ok(migrations.pop())
  }

  async fn get_applied_migrations(
    &mut self,
    migration_table_name: &str,
  ) -> Result<Vec<Migration>, Error> {
    let migrations = self
      .query(&GET_APPLIED_MIGRATIONS_QUERY.replace("%MIGRATION_TABLE_NAME%", migration_table_name))
      .await
      .migration_err("error getting applied migrations", None)?;

    Ok(migrations)
  }

  async fn migrate(
    &mut self,
    migrations: &[Migration],
    abort_divergent: bool,
    abort_missing: bool,
    grouped: bool,
    target: Target,
    migration_table_name: &str,
  ) -> Result<Report, Error> {
    self
      .execute([Self::assert_migrations_table_query(migration_table_name).as_str()].into_iter())
      .await
      .migration_err("error asserting migrations table", None)?;

    let applied_migrations = self
      .query(&GET_APPLIED_MIGRATIONS_QUERY.replace("%MIGRATION_TABLE_NAME%", migration_table_name))
      .await
      .migration_err("error getting current schema version", None)?;

    let migrations = verify_migrations(
      applied_migrations,
      migrations.to_vec(),
      abort_divergent,
      abort_missing,
    )?;

    if migrations.is_empty() {
      log::info!("no migrations to apply");
    }

    if grouped || matches!(target, Target::Fake | Target::FakeVersion(_)) {
      migrate_grouped(self, migrations, target, migration_table_name).await
    } else {
      migrate(self, migrations, target, migration_table_name).await
    }
  }
}
