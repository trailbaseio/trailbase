#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

mod error;
mod host;
mod http_runtime;
mod prefs;
pub mod sqlite_function_runtime;
mod sqlite_host;
mod util;

pub use trailbase_wasi_keyvalue::Store as KvStore;
pub use trailbase_wasm_common::component_path_to_name;

pub use crate::error::Error;
pub use crate::host::{SharedState, State};
pub use crate::http_runtime::{HttpStore, Runtime, RuntimeOptions};

pub struct InitArgs {
  pub version: Option<String>,
}

/// Helper to derive intended use of winch based on dev mode.
pub fn use_winch(dev: bool) -> bool {
  return if cfg!(target_os = "macos") {
    // https://github.com/trailbaseio/trailbase/issues/206
    false
  } else {
    dev
  };
}
