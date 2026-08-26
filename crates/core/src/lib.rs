#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod app_state;
pub mod config;
pub mod constants;
pub mod logging;
pub mod metadata;
pub mod openapi;
pub mod records;
pub mod util;

#[cfg(debug_assertions)]
pub mod test_utils;

mod admin;
mod auth;
mod backup;
mod connection;
mod data_dir;
mod email;
mod encryption;
mod extract;
mod init_error;
mod listing;
mod migrations;
mod scheduler;
mod schema_metadata;
mod server;
mod socket_address;
mod textproto;
mod transaction_recorder;

#[cfg(feature = "wasm")]
mod wasm;

#[cfg(not(feature = "wasm"))]
mod wasm {
  #[derive(Clone)]
  pub struct SqliteFunctions;

  #[derive(Clone)]
  pub struct SqliteStore;
}

#[cfg(test)]
mod test;

pub use app_state::{AppState, InitArgs};
pub use auth::User;
pub use data_dir::DataDir;
pub use init_error::InitError;
pub use server::{Server, ServerOptions};
pub use socket_address::SocketAddr;

use prost_reflect::DescriptorPool;
use std::sync::LazyLock;

static FILE_DESCRIPTOR_SET: &[u8] =
  include_bytes!(concat!(env!("OUT_DIR"), "/file_descriptor_set.bin"));

static DESCRIPTOR_POOL: LazyLock<DescriptorPool> = LazyLock::new(|| {
  DescriptorPool::decode(FILE_DESCRIPTOR_SET).expect("Failed to load file descriptor set")
});

pub mod api {
  pub use crate::admin::user::create_user::{CreateUserRequest, create_user_handler};
  pub use crate::app_state::InitArgs;
  pub use crate::auth::{AuthTokenClaims, JwtHelper, cli};
  pub use crate::backup::{Backup, backup_all, delete_backups, find_backups, restore_all};
  pub use crate::connection::Connection;
  pub use crate::email::{Email, EmailError};
  pub use crate::migrations::new_unique_migration_filename;
  pub use crate::records::json_schema::build_api_json_schema;
  pub use crate::schema_metadata::ConnectionMetadata;

  pub use trailbase_schema::json_schema::JsonSchemaMode;

  pub use crate::auth::util::{UserIdentifier, login_with_password_for_test};
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

  struct NumericAndLowerCase;

  impl SampleString for NumericAndLowerCase {
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

  impl Distribution<u8> for NumericAndLowerCase {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> u8 {
      const GEN_ASCII_STR_CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
      const RANGE: u32 = GEN_ASCII_STR_CHARSET.len() as u32;

      return GEN_ASCII_STR_CHARSET[(rng.next_u32() % RANGE) as usize];
    }
  }

  pub fn random_numeric_and_lowercase(length: usize) -> String {
    let mut rng = rand::rng();
    let _: &dyn CryptoRng = &rng;

    return NumericAndLowerCase.sample_string(&mut rng, length);
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
