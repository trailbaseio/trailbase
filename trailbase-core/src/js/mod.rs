#[cfg(feature = "v8")]
mod import_provider;

#[cfg(feature = "v8")]
mod runtime;

#[cfg(not(feature = "v8"))]
mod fallback {
  #[derive(Clone)]
  pub(crate) struct RuntimeHandle {}

  impl RuntimeHandle {
    pub(crate) fn set_connection(&self, _conn: libsql::Connection) {}

    pub(crate) fn new() -> Self {
      return Self {};
    }

    pub(crate) fn new_with_threads(n_threads: usize) -> Self {
      return Self {};
    }
  }
}

#[cfg(feature = "v8")]
pub use runtime::*;

#[cfg(not(feature = "v8"))]
pub use fallback::*;
