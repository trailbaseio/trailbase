mod backup;
mod config;
mod email;
mod error;
mod info;
mod jobs;
mod json_schema;
mod jwt;
mod logs;
mod oauth_providers;
mod parse;
mod query;
pub(crate) mod rows;
mod table;
pub(crate) mod user;
mod util;
mod wasm;

pub use error::AdminError;

use crate::app_state::AppState;
use axum::routing::{delete, get, patch, post};
use utoipa_axum::router::OpenApiRouter;

pub fn router() -> OpenApiRouter<AppState> {
  // Using the utoipa integration, we can use the on-handler metadata as the
  // source of truth for registering the routes avoiding skew.
  // Inversely, using this macro ensures that the handlers do have metadata.
  use utoipa_axum::routes;

  return OpenApiRouter::new()
    // Row actions.
    .routes(routes!(rows::list_rows::list_rows_handler))
    .routes(routes!(rows::read_files::read_files_handler))
    .routes(routes!(rows::delete_rows::delete_rows_handler))
    .routes(routes!(rows::delete_rows::delete_row_handler))
    .routes(routes!(rows::update_row::update_row_handler))
    .routes(routes!(rows::insert_row::insert_row_handler))
    // Index actions.
    .routes(routes!(table::create_index::create_index_handler))
    .routes(routes!(table::alter_index::alter_index_handler))
    .routes(routes!(table::drop_index::drop_index_handler))
    // Table actions.
    .routes(routes!(table::create_table::create_table_handler))
    .routes(routes!(table::drop_table::drop_table_handler))
    .routes(routes!(table::alter_table::alter_table_handler))
    // Table & Index actions.
    .routes(routes!(table::list_tables::list_tables_handler))
    // Config actions
    .routes(routes!(
      config::get_config::get_config_handler,
      config::update_config::update_config_handler,
    ))
    // User actions
    .routes(routes!(
      user::list_users::list_users_handler,
      user::create_user::create_user_handler,
      user::update_user::update_user_handler,
      user::delete_user::delete_user_handler,
    ))
    // Schema actions
    .routes(routes!(json_schema::list_schemas_handler))
    .routes(routes!(
      json_schema::get_api_json_schema::get_api_json_schema_handler
    ))
    // Logs
    .routes(routes!(logs::list_logs::list_logs_handler))
    // Stats
    .routes(routes!(logs::stats::fetch_stats_handler))
    // Query execution handler for the UI editor
    .route("/query", post(query::query_handler))
    // Parse handler for UI validation.
    .route("/parse", post(parse::parse_handler))
    // List available oauth providers
    .route(
      "/oauth_providers",
      get(oauth_providers::available_oauth_providers_handler),
    )
    .route("/public_key", get(jwt::get_public_key))
    .route("/info", get(info::info_handler))
    .route("/wasm", get(wasm::list_wasm_components_handler))
    .route("/wasm/install", post(wasm::install_wasm_component_handler))
    .route(
      "/wasm/uninstall",
      post(wasm::uninstall_wasm_component_handler),
    )
    .route("/jobs", get(jobs::list_jobs_handler))
    .route("/job/run", post(jobs::run_job_handler))
    .route("/backups", get(backup::list_backups_handler))
    .route("/backups/trigger", get(backup::trigger_backup_handler))
    .route("/backups/delete", delete(backup::delete_backups_handler))
    .route("/backups/restore", patch(backup::restore_backup_handler))
    .route("/email/test", post(email::test_email_handler));
}
