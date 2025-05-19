#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

mod assets;
pub mod auth;
pub mod version;

pub use assets::AssetService;

use rust_embed::RustEmbed;

#[derive(RustEmbed, Clone)]
#[folder = "js/admin/dist/"]
pub struct AdminAssets;

#[derive(RustEmbed, Clone)]
#[folder = "js/auth/dist/"]
pub struct AuthAssets;
