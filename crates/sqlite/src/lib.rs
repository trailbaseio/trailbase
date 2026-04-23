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

mod connection;
mod database;
mod error;
pub mod from_sql;
mod params;
mod rows;
pub mod sqlite;
mod statement;
pub mod to_sql;
mod value;

#[cfg(debug_assertions)]
mod experimental;

pub use connection::{
  ArcLockGuard, Connection, LockError, LockGuard, Options, SyncConnection, SyncConnectionTrait,
};
pub use database::Database;
pub use error::Error;
pub use params::{NamedParamRef, NamedParams, NamedParamsRef, Params};
pub use rows::{Row, Rows, ValueType};
pub use sqlite::transaction::Transaction;
pub use statement::Statement;
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
