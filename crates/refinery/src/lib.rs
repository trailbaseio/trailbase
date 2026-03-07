#![forbid(unsafe_code)]
#![allow(clippy::collapsible_if, clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

mod drivers;
pub mod error;
mod runner;
pub mod traits;
mod util;

pub use crate::error::Error;
pub use crate::runner::{Migration, Report, Runner, Target};
pub use crate::traits::r#async::AsyncMigrate;
pub use crate::traits::sync::Migrate;
pub use crate::util::{
  MigrationType, find_migration_files, load_sql_migrations, parse_migration_name,
};

pub use rusqlite;
