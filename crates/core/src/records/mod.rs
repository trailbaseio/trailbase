use trailbase_sqlite::ConnectionType;
use utoipa_axum::router::OpenApiRouter;

pub(crate) mod create_record;
pub(crate) mod delete_record;
pub(crate) mod files;
pub(crate) mod filter;
pub(crate) mod json_schema;
pub(crate) mod list_records;
pub(crate) mod params;
pub(crate) mod read_queries;
pub(crate) mod read_record;
pub(crate) mod subscribe;
pub(crate) mod util;
pub(crate) mod write_queries;

#[cfg(test)]
pub mod test_utils;

mod error;
mod expand;
mod record_api;
mod transaction;
mod update_record;
mod validate;

pub(crate) use error::RecordError;
pub use record_api::RecordApi;
pub(crate) use validate::validate_record_api_config;

use crate::AppState;
use crate::config::proto::PermissionFlag;

pub(crate) fn router(
  connection_type: ConnectionType,
  enable_transactions: bool,
) -> OpenApiRouter<AppState> {
  // Using the utoipa integration, we can use the on-handler metadata as the
  // source of truth for registering the routes avoiding skew.
  // Inversely, using this macro ensures that the handlers do have metadata.
  use utoipa_axum::routes;

  let mut router = OpenApiRouter::new()
    .routes(routes!(create_record::create_record_handler))
    .routes(routes!(read_record::read_record_handler))
    .routes(routes!(update_record::update_record_handler))
    .routes(routes!(delete_record::delete_record_handler))
    .routes(routes!(list_records::list_records_handler))
    .routes(routes!(read_record::get_uploaded_file_from_record_handler))
    .routes(routes!(read_record::get_uploaded_files_from_record_handler))
    .routes(routes!(json_schema::json_schema_handler));

  if matches!(connection_type, ConnectionType::Sqlite) {
    router = router.routes(routes!(
      subscribe::handler::add_subscription_sse_and_ws_handler
    ));
  }

  if enable_transactions {
    router = router.routes(routes!(transaction::record_transactions_handler));
  }

  return router;
}

// Since this is for APIs access control, we'll use the API- space CRUD terminology instead of
// database terminology.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
  // TODO: Should there be a separate "list records" permission or is "read" enough?
  Create = 1,  // ~ DB insert
  Read = 2,    // ~ DB select
  Update = 4,  // ~ DB update
  Delete = 8,  // ~ DB delete
  Schema = 16, // Lookup json schema for the given record api .
}

#[derive(Default)]
pub struct Acls {
  pub world: Vec<PermissionFlag>,
  pub authenticated: Vec<PermissionFlag>,
}

#[derive(Default)]
pub struct AccessRules {
  pub create: Option<String>,
  pub read: Option<String>,
  pub update: Option<String>,
  pub delete: Option<String>,
  pub schema: Option<String>,
}
