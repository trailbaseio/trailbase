#[cfg(feature = "v8")]
pub(crate) mod runtime;

#[cfg(not(feature = "v8"))]
mod fallback {
  #[derive(Clone)]
  pub(crate) struct RuntimeHandle {}

  impl RuntimeHandle {
    pub(crate) fn singleton() -> Self {
      return Self {};
    }

    pub(crate) fn singleton_or_init_with_threads(_: usize) -> Self {
      return Self {};
    }
  }

  pub fn register_database_functions(_: &RuntimeHandle, _: trailbase_sqlite::Connection) {}
}

#[cfg(feature = "v8")]
pub use trailbase_js::runtime::{RuntimeHandle, register_database_functions};

#[cfg(not(feature = "v8"))]
pub use fallback::*;
