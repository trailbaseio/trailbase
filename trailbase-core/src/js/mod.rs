#[cfg(feature = "v8")]
pub(crate) mod runtime;

#[cfg(not(feature = "v8"))]
mod fallback {
  #[derive(Clone)]
  pub(crate) struct RuntimeHandle {}

  impl RuntimeHandle {
    pub(crate) fn set_connection(&self, _conn: trailbase_sqlite::Connection, r#override: bool) {}

    pub(crate) fn new() -> Self {
      return Self {};
    }

    pub(crate) fn new_with_threads(n_threads: usize) -> Self {
      return Self {};
    }
  }
}

#[cfg(feature = "v8")]
pub use trailbase_js::runtime::RuntimeHandle;

#[cfg(not(feature = "v8"))]
pub use fallback::*;
