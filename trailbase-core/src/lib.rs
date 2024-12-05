#![forbid(unsafe_code)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod app_state;
pub mod assets;
pub mod config;
pub mod constants;
pub mod logging;
pub mod records;
pub mod util;

mod admin;
mod auth;
mod data_dir;
mod email;
mod extract;
mod js;
mod listing;
mod migrations;
mod scheduler;
mod schema;
mod server;
mod table_metadata;
mod transaction;
mod value_notifier;

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
        modifiers(),
        nest(
            (path = "/api/auth/v1", api = crate::auth::AuthAPI),
            (path = "/api/records/v1", api = crate::records::RecordOpenApi),
        ),
        tags()
    )]
  pub struct Doc;
}

pub mod api {
  pub use trailbase_sqlite::connect_sqlite;

  pub use crate::admin::user::{create_user_handler, CreateUserRequest};
  pub use crate::auth::api::login::login_with_password;
  pub use crate::auth::{force_password_reset, JwtHelper, TokenClaims};
  pub use crate::email::{Email, EmailError};
  pub use crate::migrations::new_unique_migration_filename;
  pub use crate::server::{init_app_state, InitArgs};
  pub use crate::table_metadata::{build_json_schema, JsonSchemaMode, TableMetadataCache};
}

pub(crate) mod rand {
  use rand::{distributions::Alphanumeric, prelude::*, rngs::OsRng};

  pub(crate) fn generate_random_string(length: usize) -> String {
    let csprng = OsRng {};
    return csprng
      .sample_iter(&Alphanumeric)
      .take(length)
      .map(char::from)
      .collect();
  }
}
