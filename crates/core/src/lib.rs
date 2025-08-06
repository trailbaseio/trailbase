#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod app_state;
pub mod config;
pub mod constants;
pub mod logging;
pub mod records;
pub mod util;

mod admin;
mod auth;
mod connection;
mod data_dir;
mod email;
mod extract;
mod js;
mod listing;
mod migrations;
mod scheduler;
mod schema_metadata;
mod server;
mod transaction;

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
  pub use crate::schema_metadata::SchemaMetadataCache;
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
