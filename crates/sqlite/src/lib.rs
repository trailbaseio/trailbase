#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(
  clippy::await_holding_lock,
  clippy::empty_enums,
  clippy::enum_glob_use,
  clippy::inefficient_to_string,
  clippy::mem_forget,
  clippy::mutex_integer,
  clippy::needless_continue
)]

mod database;
mod error;
pub mod from_sql;
mod params;
mod rows;
pub mod sqlite;
mod statement;
pub mod to_sql;
pub mod traits;
mod value;

#[cfg(feature = "pg")]
mod generic;
#[cfg(feature = "pg")]
mod pg;

#[cfg(not(feature = "generic"))]
mod connection_imports {
  pub use super::sqlite::connection::{ArcLockGuard, Connection, LockError, LockGuard, Options};
  pub use super::sqlite::sync::SyncConnection;
}

#[cfg(feature = "generic")]
mod connection_imports {
  pub use super::generic::{Connection, SyncConnection};
  pub use super::sqlite::connection::{ArcLockGuard, LockError, LockGuard, Options};
}

pub use connection_imports::*;

pub use database::Database;
pub use error::Error;
pub use params::{NamedParamRef, NamedParams, NamedParamsRef, Params};
pub use rows::{Row, Rows, ValueType};
pub use sqlite::transaction::Transaction;
pub use statement::Statement;
pub use traits::SyncConnection as SyncConnectionTrait;
pub use value::{Value, ValueRef};

#[macro_export]
macro_rules! params {
    () => {
        [] as [$crate::to_sql::ToSqlProxy]
    };
    ($($param:expr),+ $(,)?) => {
        [$(Into::<$crate::to_sql::ToSqlProxy>::into($param)),+]
    };
}

#[macro_export]
macro_rules! named_params {
    () => {
        [] as [(&str, $crate::to_sql::ToSqlProxy)]
    };
    ($($param_name:literal: $param_val:expr),+ $(,)?) => {
        [$(($param_name as &str, Into::<$crate::to_sql::ToSqlProxy>::into($param_val))),+]
    };
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
