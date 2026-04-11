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

pub mod connection;
pub mod error;
pub mod params;
pub mod rows;
pub mod sqlite;
pub mod to_sql;
pub mod value;

pub use connection::Connection;
pub use error::Error;
pub use params::{NamedParamRef, NamedParams, NamedParamsRef, Params};
pub use rows::{Row, Rows, ValueType};
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
