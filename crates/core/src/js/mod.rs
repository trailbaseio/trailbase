#[cfg(feature = "v8")]
pub(crate) mod runtime;

#[cfg(feature = "v8")]
pub use trailbase_js::runtime::{RuntimeHandle, register_database_functions};
