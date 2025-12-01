use crate::{Migration, Report};
use std::fmt;
use std::path::PathBuf;
use thiserror::Error as TError;

/// An Error occurred during a migration cycle
#[derive(Debug)]
pub struct Error {
  kind: Box<Kind>,
  report: Option<Report>,
}

impl Error {
  /// Instantiate a new Error
  pub fn new(kind: Kind, report: Option<Report>) -> Error {
    Error {
      kind: Box::new(kind),
      report,
    }
  }

  /// Return the Report of the migration cycle if any
  pub fn report(&self) -> Option<&Report> {
    self.report.as_ref()
  }

  /// Return the kind of error occurred
  pub fn kind(&self) -> &Kind {
    &self.kind
  }
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.kind)
  }
}

impl std::error::Error for Error {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    self.kind.source()
  }
}

/// Enum listing possible errors from Refinery.
#[derive(Debug, TError)]
pub enum Kind {
  /// An Error from an invalid file name migration
  #[error("migration name must be in the format V{{number}}__{{name}}")]
  InvalidName,
  /// An Error from an invalid version on a file name migration
  #[error("migration version must be a valid integer")]
  InvalidVersion,
  /// An Error from a repeated version, migration version numbers must be unique
  #[error("migration {0} is repeated, migration versions must be unique")]
  RepeatedVersion(Migration),
  /// An Error from an divergent version, the applied version is different to the filesystem one
  #[error("applied migration {0} is different than filesystem one {1}")]
  DivergentVersion(Migration, Migration),
  /// An Error from an divergent version, the applied version is missing on the filesystem
  #[error("migration {0} is missing from the filesystem")]
  MissingVersion(Migration),
  /// An Error from an invalid migrations path location
  #[error("invalid migrations path {0}, {1}")]
  InvalidMigrationPath(PathBuf, std::io::Error),
  /// An Error parsing refinery Config
  #[error("Error parsing config: {0}")]
  ConfigError(String),
  /// An Error from an underlying database connection Error
  #[error("`{0}`, `{1}`")]
  Connection(String, #[source] Box<dyn std::error::Error + Sync + Send>),
  /// An Error from an invalid migration file (not UTF-8 etc)
  #[error("invalid migration file at path {0}, {1}")]
  InvalidMigrationFile(PathBuf, std::io::Error),
}

// Helper trait for adding custom messages and applied migrations to Connection error's.
pub trait WrapMigrationError<T, E> {
  fn migration_err(self, msg: &str, report: Option<&[Migration]>) -> Result<T, Error>;
}

impl<T, E> WrapMigrationError<T, E> for Result<T, E>
where
  E: std::error::Error + Send + Sync + 'static,
{
  fn migration_err(self, msg: &str, applied_migrations: Option<&[Migration]>) -> Result<T, Error> {
    match self {
      Ok(report) => Ok(report),
      Err(err) => Err(Error {
        kind: Box::new(Kind::Connection(msg.into(), Box::new(err))),
        report: applied_migrations.map(|am| Report::new(am.to_vec())),
      }),
    }
  }
}
