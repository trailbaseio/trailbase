#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(
  clippy::await_holding_lock,
  clippy::empty_enum,
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

pub use rusqlite::types::Value;

pub use connection::Connection;
pub use error::Error;
pub use params::{NamedParamRef, NamedParams, NamedParamsRef, Params};
pub use rows::{Row, Rows, ValueType};
