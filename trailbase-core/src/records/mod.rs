use axum::{
  Router,
  routing::{delete, get, patch, post},
};
use utoipa::OpenApi;

pub(crate) mod create_record;
pub(crate) mod delete_record;
mod error;
pub(crate) mod files;
pub(crate) mod json_schema;
pub(crate) mod list_records;
pub(crate) mod params;
pub mod query_builder;
pub(crate) mod read_record;
mod record_api;
pub mod sql_to_json;
pub(crate) mod subscribe;
pub mod test_utils;
mod update_record;
mod validate;

pub(crate) use error::RecordError;
pub use record_api::RecordApi;
pub(crate) use validate::validate_record_api_config;

use crate::AppState;
use crate::config::proto::PermissionFlag;
use crate::constants::RECORD_API_PATH;

#[allow(unused)]
#[derive(OpenApi)]
#[openapi(
  paths(
    read_record::read_record_handler,
    read_record::get_uploaded_file_from_record_handler,
    read_record::get_uploaded_files_from_record_handler,
    list_records::list_records_handler,
    create_record::create_record_handler,
    update_record::update_record_handler,
    delete_record::delete_record_handler,
    json_schema::json_schema_handler,
  ),
  components(schemas(create_record::CreateRecordResponse))
)]
pub(super) struct RecordOpenApi;

pub(crate) fn router() -> Router<AppState> {
  return Router::new()
    .route(
      &format!("/{RECORD_API_PATH}/{{name}}/{{record}}"),
      get(read_record::read_record_handler),
    )
    .route(
      &format!("/{RECORD_API_PATH}/{{name}}"),
      post(create_record::create_record_handler),
    )
    .route(
      &format!("/{RECORD_API_PATH}/{{name}}/{{record}}"),
      patch(update_record::update_record_handler),
    )
    .route(
      &format!("/{RECORD_API_PATH}/{{name}}/{{record}}"),
      delete(delete_record::delete_record_handler),
    )
    .route(
      &format!("/{RECORD_API_PATH}/{{name}}"),
      get(list_records::list_records_handler),
    )
    .route(
      &format!("/{RECORD_API_PATH}/{{name}}/{{record}}/file/{{column_name}}"),
      get(read_record::get_uploaded_file_from_record_handler),
    )
    .route(
      &format!("/{RECORD_API_PATH}/{{name}}/{{record}}/files/{{column_name}}/{{file_index}}"),
      get(read_record::get_uploaded_files_from_record_handler),
    )
    .route(
      &format!("/{RECORD_API_PATH}/{{name}}/schema"),
      get(json_schema::json_schema_handler),
    )
    .route(
      &format!("/{RECORD_API_PATH}/{{name}}/subscribe/{{record}}"),
      get(subscribe::add_subscription_sse_handler),
    );
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
