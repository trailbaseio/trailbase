use axum::{
  routing::{delete, get, patch, post},
  Router,
};
use utoipa::OpenApi;

pub(crate) mod create_record;
pub(crate) mod delete_record;
mod error;
pub(crate) mod files;
mod json_schema;
pub mod json_to_sql;
mod list_records;
pub(crate) mod read_record;
mod record_api;
pub mod sql_to_json;
pub mod test_utils;
mod update_record;
mod validate;

pub(crate) use error::RecordError;
pub use record_api::RecordApi;
pub(crate) use validate::validate_record_api_config;

use crate::config::proto::{PermissionFlag, RecordApiConfig};
use crate::config::ConfigError;
use crate::AppState;

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
    .route("/:name/:record", get(read_record::read_record_handler))
    .route("/:name", post(create_record::create_record_handler))
    .route(
      "/:name/:record",
      patch(update_record::update_record_handler),
    )
    .route(
      "/:name/:record",
      delete(delete_record::delete_record_handler),
    )
    .route("/:name", get(list_records::list_records_handler))
    .route(
      "/:name/:record/file/:column_name",
      get(read_record::get_uploaded_file_from_record_handler),
    )
    .route(
      "/:name/:record/files/:column_name/:file_index",
      get(read_record::get_uploaded_files_from_record_handler),
    )
    .route("/:name/schema", get(json_schema::json_schema_handler));
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

// NOTE: used in integration test.
pub async fn add_record_api(
  state: &AppState,
  api_name: &str,
  table_name: &str,
  acls: Acls,
  access_rules: AccessRules,
) -> Result<(), ConfigError> {
  let mut config = state.get_config();

  config.record_apis.push(RecordApiConfig {
    name: Some(api_name.to_string()),
    table_name: Some(table_name.to_string()),

    acl_world: acls.world.into_iter().map(|f| f as i32).collect(),
    acl_authenticated: acls.authenticated.into_iter().map(|f| f as i32).collect(),
    conflict_resolution: None,
    autofill_missing_user_id_columns: Some(false),
    create_access_rule: access_rules.create,
    read_access_rule: access_rules.read,
    update_access_rule: access_rules.update,
    delete_access_rule: access_rules.delete,
    schema_access_rule: access_rules.schema,
  });

  return state.validate_and_update_config(config, None).await;
}
