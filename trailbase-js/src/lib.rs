#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod import_provider;
pub mod runtime;
mod util;

pub use crate::import_provider::ImportProvider;

#[derive(rust_embed::RustEmbed, Clone)]
#[folder = "assets/runtime/dist/"]
pub struct JsRuntimeAssets;
