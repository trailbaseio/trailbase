//! A client library to connect to a TrailBase server via HTTP.
//!
//! TrailBase is a sub-millisecond, open-source application server with type-safe APIs, built-in
//! WASM runtime, realtime, auth, and admin UI built on Rust, SQLite & Wasmtime.

#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]
mod client;
mod error;
mod record_api;
mod transport;

pub use crate::client::*;
pub use crate::error::Error;
pub use crate::record_api::*;
pub use crate::transport::{DefaultTransport, Transport};
pub use futures_lite::Stream;
