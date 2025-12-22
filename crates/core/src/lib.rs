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

  #[derive(Clone)]
  pub(crate) struct KvStore;

  impl KvStore {
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

  #[derive(Clone)]
  pub struct SqliteFunctionRuntime;

  pub struct WasmRuntimeResult {
    pub shared_kv_store: KvStore,
    pub build_wasm_runtime: Box<dyn Fn() -> Result<Vec<Runtime>, AnyError> + Send + Sync>,
  }

  pub fn build_wasm_runtime(
    _data_dir: DataDir,
    _conn: trailbase_sqlite::Connection,
    _runtime_root_fs: Option<std::path::PathBuf>,
    _runtime_threads: Option<usize>,
    _dev: bool,
  ) -> Result<WasmRuntimeResult, AnyError> {
    return Ok(WasmRuntimeResult {
      shared_kv_store: KvStore,
      build_wasm_runtime: Box::new(|| Ok(vec![])),
    });
  }

  pub fn build_sync_wasm_runtimes_for_components(
    _components_path: PathBuf,
    _fs_root_path: Option<&std::path::Path>,
    _dev: bool,
  ) -> Result<Vec<SqliteFunctionRuntime>, AnyError> {
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
  pub use crate::auth::util::login_with_password;
  pub use crate::auth::{JwtHelper, TokenClaims, cli};
  pub use crate::connection::{Connection, init_main_db};
  pub use crate::email::{Email, EmailError};
  pub use crate::migrations::new_unique_migration_filename;
  pub use crate::records::json_schema::build_api_json_schema;
  pub use crate::schema_metadata::ConnectionMetadata;
  pub use crate::server::{InitArgs, init_app_state, serve};

  pub use trailbase_schema::json_schema::JsonSchemaMode;
}

pub(crate) mod rand {
  use rand::{
    CryptoRng,
    distr::{Alphanumeric, SampleString},
  };

  pub(crate) fn generate_random_string(length: usize) -> String {
    let mut rng = rand::rng();
    let _: &dyn CryptoRng = &rng;

    return Alphanumeric.sample_string(&mut rng, length);
  }

  #[cfg(test)]
  mod tests {
    use super::*;

    #[test]
    fn test_generate_random_string() {
      let n = 20;
      let first = generate_random_string(20);
      assert_eq!(n, first.len());
      let second = generate_random_string(20);
      assert_eq!(n, second.len());
      assert_ne!(first, second);
    }
  }
}
