use siphasher::sip::SipHasher13;
use time::OffsetDateTime;

use log::error;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::fmt;
use std::hash::{Hash, Hasher};

use crate::traits::{DEFAULT_MIGRATION_TABLE_NAME, sync::migrate as sync_migrate};
use crate::util::parse_migration_name;
use crate::{AsyncMigrate, Error, Migrate};
use std::fmt::Formatter;

/// An enum set that represents the type of the Migration
#[derive(Clone, PartialEq)]
pub enum Type {
  Versioned,
  Unversioned,
}

impl fmt::Display for Type {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    let version_type = match self {
      Type::Versioned => "V",
      Type::Unversioned => "U",
    };
    write!(f, "{}", version_type)
  }
}

impl fmt::Debug for Type {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    let version_type = match self {
      Type::Versioned => "Versioned",
      Type::Unversioned => "Unversioned",
    };
    write!(f, "{}", version_type)
  }
}

/// An enum set that represents the target version up to which refinery should migrate, it is used
/// by [Runner]
#[derive(Clone, Copy, Debug)]
pub enum Target {
  Latest,
  Version(u32),
  Fake,
  FakeVersion(u32),
}

// an Enum set that represents the state of the migration: Applied on the database,
// or Unapplied yet to be applied on the database
#[derive(Clone, Debug)]
enum State {
  Applied,
  Unapplied,
}

/// Represents a schema migration to be run on the database,
/// this struct is used by the [`embed_migrations!`] macro to gather migration files
/// and shouldn't be needed by the user
///
/// [`embed_migrations!`]: macro.embed_migrations.html
#[derive(Clone, Debug)]
pub struct Migration {
  state: State,
  name: String,
  checksum: u64,
  version: i32,
  prefix: Type,
  sql: Option<String>,
  applied_on: Option<OffsetDateTime>,
}

impl Migration {
  /// Create an unapplied migration, name and version are parsed from the input_name,
  /// which must be named in the format (U|V){1}__{2}.rs where {1} represents the migration version
  /// and {2} the name.
  pub fn unapplied(input_name: &str, sql: &str) -> Result<Migration, Error> {
    let (prefix, version, name) = parse_migration_name(input_name)?;

    // Previously, `std::collections::hash_map::DefaultHasher` was used
    // to calculate the checksum and the implementation at that time
    // was SipHasher13. However, that implementation is not guaranteed:
    // > The internal algorithm is not specified, and so it and its
    // > hashes should not be relied upon over releases.
    // We now explicitly use SipHasher13 to both remain compatible with
    // existing migrations and prevent breaking from possible future
    // changes to `DefaultHasher`.
    let mut hasher = SipHasher13::new();
    name.hash(&mut hasher);
    version.hash(&mut hasher);
    sql.hash(&mut hasher);
    let checksum = hasher.finish();

    Ok(Migration {
      state: State::Unapplied,
      name,
      version,
      prefix,
      sql: Some(sql.into()),
      applied_on: None,
      checksum,
    })
  }

  // Create a migration from an applied migration on the database
  pub fn applied(
    version: i32,
    name: String,
    applied_on: OffsetDateTime,
    checksum: u64,
  ) -> Migration {
    Migration {
      state: State::Applied,
      name,
      checksum,
      version,
      // applied migrations are always versioned
      prefix: Type::Versioned,
      sql: None,
      applied_on: Some(applied_on),
    }
  }

  // convert the Unapplied into an Applied Migration
  pub fn set_applied(&mut self) {
    self.applied_on = Some(OffsetDateTime::now_utc());
    self.state = State::Applied;
  }

  // Get migration sql content
  pub fn sql(&self) -> Option<&str> {
    self.sql.as_deref()
  }

  /// Get the Migration version
  pub fn version(&self) -> u32 {
    self.version as u32
  }

  /// Get the Prefix
  pub fn prefix(&self) -> &Type {
    &self.prefix
  }

  /// Get the Migration Name
  pub fn name(&self) -> &str {
    &self.name
  }

  /// Get the timestamp from when the Migration was applied. `None` when unapplied.
  /// Migrations returned from Runner::get_migrations() will always have `None`.
  pub fn applied_on(&self) -> Option<&OffsetDateTime> {
    self.applied_on.as_ref()
  }

  /// Get the Migration checksum. Checksum is formed from the name version and sql of the Migration
  pub fn checksum(&self) -> u64 {
    self.checksum
  }
}

impl fmt::Display for Migration {
  fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(fmt, "{}{}__{}", self.prefix, self.version, self.name)
  }
}

impl Eq for Migration {}

impl PartialEq for Migration {
  fn eq(&self, other: &Migration) -> bool {
    self.version == other.version && self.name == other.name && self.checksum() == other.checksum()
  }
}

impl Ord for Migration {
  fn cmp(&self, other: &Migration) -> Ordering {
    self.version.cmp(&other.version)
  }
}

impl PartialOrd for Migration {
  fn partial_cmp(&self, other: &Migration) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

/// Struct that represents the report of the migration cycle,
/// a `Report` instance is returned by the [`Runner::run`] and [`Runner::run_async`] methods
/// via [`Result`]`<Report, Error>`, on case of an [`Error`] during a migration, you can access the
/// `Report` with [`Error.report`]
///
/// [`Error`]: struct.Error.html
/// [`Runner::run`]: struct.Runner.html#method.run
/// [`Runner::run_async`]: struct.Runner.html#method.run_async
/// [`Result`]: https://doc.rust-lang.org/std/result/enum.Result.html
/// [`Error.report`]:  struct.Error.html#method.report
#[derive(Clone, Debug)]
pub struct Report {
  applied_migrations: Vec<Migration>,
}

impl Report {
  /// Instantiate a new Report
  pub(crate) fn new(applied_migrations: Vec<Migration>) -> Report {
    Report { applied_migrations }
  }

  /// Retrieves the list of applied `Migration` of the migration cycle
  pub fn applied_migrations(&self) -> &Vec<Migration> {
    &self.applied_migrations
  }
}

/// Struct that represents the entrypoint to run the migrations,
/// an instance of this struct is returned by the [`embed_migrations!`] macro.
/// `Runner` should not need to be instantiated manually
///
/// [`embed_migrations!`]: macro.embed_migrations.html
pub struct Runner {
  grouped: bool,
  abort_divergent: bool,
  abort_missing: bool,
  migrations: Vec<Migration>,
  target: Target,
  migration_table_name: String,
}

impl Runner {
  /// instantiate a new Runner
  pub fn new(migrations: &[Migration]) -> Runner {
    Runner {
      grouped: false,
      target: Target::Latest,
      abort_divergent: true,
      abort_missing: true,
      migrations: migrations.to_vec(),
      migration_table_name: DEFAULT_MIGRATION_TABLE_NAME.into(),
    }
  }

  /// Get the gathered migrations.
  pub fn get_migrations(&self) -> &Vec<Migration> {
    &self.migrations
  }

  /// Set the target version up to which refinery should migrate, Latest migrates to the latest
  /// version available Version migrates to a user provided version, a Version with a higher
  /// version than the latest will be ignored, and Fake doesn't actually run any migration, just
  /// creates and updates refinery's schema migration table by default this is set to Latest
  pub fn set_target(self, target: Target) -> Runner {
    Runner { target, ..self }
  }

  /// Set true if all migrations should be grouped and run in a single transaction.
  /// by default this is set to false, each migration runs on their own transaction
  ///
  /// # Note
  ///
  /// set_grouped won't probably work on MySQL Databases as MySQL lacks support for transactions
  /// around schema alteration operations, meaning that if a migration fails to apply you will
  /// have to manually unpick the changes in order to try again (it’s impossible to roll back to an
  /// earlier point).
  pub fn set_grouped(self, grouped: bool) -> Runner {
    Runner { grouped, ..self }
  }

  /// Set true if migration process should abort if divergent migrations are found
  /// i.e. applied migrations with the same version but different name or checksum from the ones on
  /// the filesystem. by default this is set to true
  pub fn set_abort_divergent(self, abort_divergent: bool) -> Runner {
    Runner {
      abort_divergent,
      ..self
    }
  }

  /// Set true if migration process should abort if missing migrations are found
  /// i.e. applied migrations that are not found on the filesystem,
  /// or migrations found on filesystem with a version inferior to the last one applied but not
  /// applied. by default this is set to true
  pub fn set_abort_missing(self, abort_missing: bool) -> Runner {
    Runner {
      abort_missing,
      ..self
    }
  }

  /// Queries the database for the last applied migration, returns None if there aren't applied
  /// Migrations
  pub fn get_last_applied_migration<C>(&self, conn: &'_ mut C) -> Result<Option<Migration>, Error>
  where
    C: Migrate,
  {
    Migrate::get_last_applied_migration(conn, &self.migration_table_name)
  }

  /// Queries the database asynchronously for the last applied migration, returns None if there
  /// aren't applied Migrations
  pub async fn get_last_applied_migration_async<C>(
    &self,
    conn: &mut C,
  ) -> Result<Option<Migration>, Error>
  where
    C: AsyncMigrate + Send,
  {
    AsyncMigrate::get_last_applied_migration(conn, &self.migration_table_name).await
  }

  /// Queries the database for all previous applied migrations
  pub fn get_applied_migrations<C>(&self, conn: &'_ mut C) -> Result<Vec<Migration>, Error>
  where
    C: Migrate,
  {
    Migrate::get_applied_migrations(conn, &self.migration_table_name)
  }

  /// Queries the database asynchronously for all previous applied migrations
  pub async fn get_applied_migrations_async<C>(&self, conn: &mut C) -> Result<Vec<Migration>, Error>
  where
    C: AsyncMigrate + Send,
  {
    AsyncMigrate::get_applied_migrations(conn, &self.migration_table_name).await
  }

  /// Set the table name to use for the migrations table. The default name is
  /// `refinery_schema_history`
  ///
  /// ### Warning
  /// Changing this can be disastrous for your database. You should verify that the migrations table
  /// has the same name as the name you specify here, if this is changed on an existing project.
  ///
  /// # Panics
  ///
  /// If the provided `migration_table_name` is empty
  pub fn set_migration_table_name<S: AsRef<str>>(&mut self, migration_table_name: S) -> &mut Self {
    if migration_table_name.as_ref().is_empty() {
      panic!("Migration table name must not be empty");
    }

    self.migration_table_name = migration_table_name.as_ref().to_string();
    self
  }

  /// Creates an iterator over pending migrations, applying each before returning
  /// the result from `next()`. If a migration fails, the iterator will return that
  /// result and further calls to `next()` will return `None`.
  pub fn run_iter<C>(
    self,
    connection: &mut C,
  ) -> impl Iterator<Item = Result<Migration, Error>> + '_
  where
    C: Migrate,
  {
    RunIterator::new(self, connection)
  }

  /// Runs the Migrations in the supplied database connection
  pub fn run<C>(&self, connection: &mut C) -> Result<Report, Error>
  where
    C: Migrate,
  {
    Migrate::migrate(
      connection,
      &self.migrations,
      self.abort_divergent,
      self.abort_missing,
      self.grouped,
      self.target,
      &self.migration_table_name,
    )
  }

  /// Runs the Migrations asynchronously in the supplied database connection
  pub async fn run_async<C>(&self, connection: &mut C) -> Result<Report, Error>
  where
    C: AsyncMigrate + Send,
  {
    AsyncMigrate::migrate(
      connection,
      &self.migrations,
      self.abort_divergent,
      self.abort_missing,
      self.grouped,
      self.target,
      &self.migration_table_name,
    )
    .await
  }
}

pub struct RunIterator<'a, C> {
  connection: &'a mut C,
  target: Target,
  migration_table_name: String,
  items: VecDeque<Migration>,
  failed: bool,
}
impl<'a, C> RunIterator<'a, C>
where
  C: Migrate,
{
  pub(crate) fn new(runner: Runner, connection: &'a mut C) -> RunIterator<'a, C> {
    RunIterator {
      items: VecDeque::from(
        Migrate::get_unapplied_migrations(
          connection,
          &runner.migrations,
          runner.abort_divergent,
          runner.abort_missing,
          &runner.migration_table_name,
        )
        .unwrap(),
      ),
      connection,
      target: runner.target,
      migration_table_name: runner.migration_table_name.clone(),
      failed: false,
    }
  }
}
impl<C> Iterator for RunIterator<'_, C>
where
  C: Migrate,
{
  type Item = Result<Migration, Error>;

  fn next(&mut self) -> Option<Self::Item> {
    match self.failed {
      true => None,
      false => self.items.pop_front().and_then(|migration| {
        sync_migrate(
          self.connection,
          vec![migration],
          self.target,
          &self.migration_table_name,
          false,
        )
        .map(|r| r.applied_migrations.first().cloned())
        .map_err(|e| {
          error!("migration failed: {e:?}");
          self.failed = true;
          e
        })
        .transpose()
      }),
    }
  }
}
