#![allow(clippy::needless_return)]
#![warn(
  unsafe_code,
  clippy::await_holding_lock,
  clippy::empty_enum,
  clippy::enum_glob_use,
  clippy::inefficient_to_string,
  clippy::mem_forget,
  clippy::mutex_integer,
  clippy::needless_continue
)]

mod extension;

pub mod connection;
pub mod error;
pub mod geoip;
pub mod params;
mod rows;
pub mod schema;

pub use connection::Connection;
pub use error::Error;
pub use extension::connect_sqlite;
pub use params::{NamedParamRef, NamedParams, NamedParamsRef, Params};
pub use rows::{Row, Rows, ValueType};
pub use rusqlite::types::Value;
