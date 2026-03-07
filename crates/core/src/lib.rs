#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod app_state;
pub mod config;
pub mod constants;
pub mod logging;
pub mod metadata;
pub mod records;
pub mod util;

mod admin;
mod auth;
mod connection;
mod data_dir;
mod email;
mod encryption;
mod extract;
mod listing;
mod migrations;
mod scheduler;
mod schema_metadata;
mod server;
mod transaction;

#[cfg(feature = "wasm")]
mod wasm;

#[cfg(not(feature = "wasm"))]
mod wasm {
  use axum::Router;
  use std::path::PathBuf;
  use std::sync::Arc;
  use tokio::sync::RwLock;

  use crate::{AppState, DataDir};

  pub(crate) type AnyError = Box<dyn std::error::Error + Send + Sync>;

  #[derive(Clone, Default)]
  pub(crate) struct KvStore;

  impl KvStore {
    pub(crate) fn new() -> Self {
      return Self::default();
    }

    pub(crate) fn set(&self, _key: String, _value: Vec<u8>) -> Option<Vec<u8>> {
      return None;
    }
  }

  pub(crate) struct Runtime;

  impl Runtime {
    pub fn component_path(&self) -> std::path::PathBuf {
      return std::path::PathBuf::default();
    }
  }

  pub(crate) type WasmRuntimeBuilder =
    Box<dyn Fn() -> Result<Vec<Runtime>, crate::wasm::AnyError> + Send + Sync>;

  pub(crate) fn wasm_runtimes_builder(
    _data_dir: DataDir,
    _conn: trailbase_sqlite::Connection,
    _rt: Option<tokio::runtime::Handle>,
    _runtime_root_fs: Option<std::path::PathBuf>,
    _shared_kv_store: Option<KvStore>,
    _dev: bool,
  ) -> Result<WasmRuntimeBuilder, AnyError> {
    return Ok(Box::new(|| Ok(vec![])));
  }

  #[derive(Clone)]
  pub struct SqliteFunctions;

  #[derive(Clone)]
  pub struct SqliteStore;

  pub(crate) async fn build_sync_wasm_runtimes_for_components(
    _components_path: PathBuf,
    _fs_root_path: Option<&std::path::Path>,
    _dev: bool,
  ) -> Result<Vec<(SqliteStore, SqliteFunctions)>, AnyError> {
    return Ok(vec![]);
  }

  #[cfg(not(feature = "wasm"))]
  pub(crate) async fn install_routes_and_jobs(
    _state: &AppState,
    _runtime: Arc<RwLock<Runtime>>,
  ) -> Result<Option<Router<AppState>>, AnyError> {
    return Ok(None);
  }
}

#[cfg(test)]
mod test;

pub use app_state::AppState;
pub use auth::User;
pub use data_dir::DataDir;
pub use server::{InitError, Server, ServerOptions};

use prost_reflect::DescriptorPool;
use std::sync::LazyLock;

static FILE_DESCRIPTOR_SET: &[u8] =
  include_bytes!(concat!(env!("OUT_DIR"), "/file_descriptor_set.bin"));

static DESCRIPTOR_POOL: LazyLock<DescriptorPool> = LazyLock::new(|| {
  DescriptorPool::decode(FILE_DESCRIPTOR_SET).expect("Failed to load file descriptor set")
});

pub mod openapi {
  use utoipa::OpenApi;

  #[derive(OpenApi)]
  #[openapi(
        info(
            title = "TrailBase",
            description = "TrailBase APIs",
        ),
        nest(
            (path = "/api/auth/v1", api = crate::auth::AuthApi),
            (path = "/api/records/v1", api = crate::records::RecordOpenApi),
        ),
        tags(),
    )]
  pub struct Doc;
}

pub mod api {
  pub use crate::admin::user::{CreateUserRequest, create_user_handler};
  pub use crate::auth::{AuthTokenClaims, JwtHelper, cli};
  pub use crate::connection::{Connection, init_main_db, init_session_db};
  pub use crate::email::{Email, EmailError};
  pub use crate::migrations::new_unique_migration_filename;
  pub use crate::records::json_schema::build_api_json_schema;
  pub use crate::schema_metadata::ConnectionMetadata;
  pub use crate::server::{InitArgs, init_app_state, serve};

  pub use trailbase_schema::json_schema::JsonSchemaMode;

  pub use crate::auth::util::login_with_password_for_test;
}

pub(crate) mod rand {
  use rand::distr::{Alphanumeric, Distribution, SampleString};
  use rand::{CryptoRng, Rng};

  pub fn random_alphanumeric(length: usize) -> String {
    let mut rng = rand::rng();
    let _: &dyn CryptoRng = &rng;

    return Alphanumeric.sample_string(&mut rng, length);
  }

  struct NumericAndUpperCase;

  impl SampleString for NumericAndUpperCase {
    fn append_string<R: Rng + ?Sized>(&self, rng: &mut R, string: &mut String, len: usize) {
      for c in self
        .sample_iter(rng)
        .take(len)
        .inspect(|b| debug_assert!(b.is_ascii_alphanumeric()))
      {
        string.push(char::from_u32(c as u32).expect("invariant"));
      }
    }
  }

  impl Distribution<u8> for NumericAndUpperCase {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> u8 {
      const GEN_ASCII_STR_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
      const RANGE: u32 = GEN_ASCII_STR_CHARSET.len() as u32;

      return GEN_ASCII_STR_CHARSET[(rng.next_u32() % RANGE) as usize];
    }
  }

  pub fn random_numeric_and_uppercase(length: usize) -> String {
    let mut rng = rand::rng();
    let _: &dyn CryptoRng = &rng;

    return NumericAndUpperCase.sample_string(&mut rng, length);
  }

  #[cfg(test)]
  mod tests {
    use super::*;

    #[test]
    fn test_random_alphanumeric() {
      let n = 50;
      let first = random_alphanumeric(n);
      assert_eq!(n, first.len());
      for c in first.chars() {
        assert!(c.is_alphanumeric());
      }

      let second = random_alphanumeric(n);
      assert_eq!(n, second.len());
      assert_ne!(first, second);
    }

    #[test]
    fn test_random_numberic_and_uppercase() {
      let n = 50;
      let first = random_numeric_and_uppercase(n);
      assert_eq!(n, first.len());
      for c in first.chars() {
        assert!(c.is_uppercase() || c.is_numeric());
      }

      let second = random_numeric_and_uppercase(n);
      assert_eq!(n, second.len());
      assert_ne!(first, second);
    }
  }
}
