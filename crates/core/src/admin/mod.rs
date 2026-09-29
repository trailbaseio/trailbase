mod backup;
mod config;
mod email;
mod error;
mod info;
mod jobs;
mod json_schema;
mod jwt;
mod logs;
mod mint;
mod oauth_providers;
mod openapi;
mod parse;
mod query;
pub(crate) mod rows;
mod table;
pub(crate) mod user;
mod util;
mod wasm;

pub use error::AdminError;
pub(super) use openapi::OpenApiExtension;

use axum::RequestExt;
use axum::body::Body;
use axum::extract::{Extension, Request, State};
use axum::response::Response;
use bytes::Bytes;
use std::borrow::Cow;
use trailbase_assets::AssetService;
use utoipa_axum::router::OpenApiRouter;

use crate::app_state::AppState;
use crate::auth::util::is_admin;
use crate::auth::{AuthError, User};
use crate::constants::{ADMIN_API_PATH, HEADER_CSRF_TOKEN};

pub fn unprotected_admin_router() -> OpenApiRouter<AppState> {
  // Using the utoipa integration, we can use the on-handler metadata as the
  // source of truth for registering the routes avoiding skew.
  // Inversely, using this macro ensures that the handlers do have metadata.
  use utoipa_axum::routes;

  let router = OpenApiRouter::new()
    // Row actions.
    .routes(routes!(rows::list_rows::list_rows_handler))
    .routes(routes!(rows::read_files::read_files_handler))
    .routes(routes!(rows::delete_rows::delete_rows_handler))
    .routes(routes!(rows::delete_rows::delete_row_handler))
    .routes(routes!(rows::update_row::update_row_handler))
    .routes(routes!(rows::insert_row::insert_row_handler))
    // Index actions.
    .routes(routes!(
      table::create_index::create_index_handler,
      table::alter_index::alter_index_handler,
      table::drop_index::drop_index_handler,
    ))
    // Table actions.
    .routes(routes!(
      table::create_table::create_table_handler,
      table::alter_table::alter_table_handler,
      table::drop_table::drop_table_handler,
    ))
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
    .routes(routes!(mint::mint_auth_tokens))
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
    .routes(routes!(query::query_handler))
    // Parse handler for UI validation.
    .routes(routes!(parse::parse_handler))
    // List available oauth providers
    .routes(routes!(oauth_providers::available_oauth_providers_handler))
    // Wasm component management.
    .routes(routes!(wasm::list_wasm_components_handler))
    .routes(routes!(wasm::install_wasm_component_handler))
    .routes(routes!(wasm::uninstall_wasm_component_handler))
    // Jobs
    .routes(routes!(jobs::list_jobs::list_jobs_handler))
    .routes(routes!(jobs::run_job::run_job_handler))
    // Backup routes
    .routes(routes!(backup::list_backups_handler))
    .routes(routes!(backup::trigger_backup_handler))
    .routes(routes!(backup::delete_backups_handler))
    .routes(routes!(backup::restore_backup_handler))
    // Misc:
    .routes(routes!(jwt::get_public_key))
    .routes(routes!(info::info_handler))
    .routes(routes!(openapi::openapi_handler))
    .routes(routes!(email::test_email_handler));

  OpenApiRouter::new().nest(&format!("/{ADMIN_API_PATH}/"), router)
}

pub fn protected_admin_router_and_assets(
  state: &AppState,
  openapi_ext: &OpenApiExtension,
) -> OpenApiRouter<AppState> {
  return unprotected_admin_router()
    .layer(Extension(openapi_ext.clone()))
    .layer(axum::middleware::from_fn_with_state(
      state.clone(),
      assert_admin_api_access,
    ))
    // NOTE: We cannot ACL-lock the UI assets. We need to be able to sign into the SPA.
    .nest_service(
      "/_/admin",
      AssetService::<trailbase_assets::AdminAssets>::with_parameters(
        |_path: &str| -> Option<Response<Body>> {
          // SPA fallback.
          let file = trailbase_assets::AdminAssets::get("index.html")?;

          return Some(
            Response::builder()
              .header(axum::http::header::CONTENT_TYPE, file.metadata.mimetype())
              .body(Body::from(cow_to_bytes(file.data)))
              .unwrap_or_default(),
          );
        },
      ),
    );
}

/// Assert that the caller is an admin and provides a valid CSRF token. Unlike the access to the
/// HTML/js assets, this one errors.
///
/// NOTE: returning a redirect (like below) only makes sense for the html serving, not the APIs.
async fn assert_admin_api_access(
  State(state): State<AppState>,
  mut req: Request,
  next: axum::middleware::Next,
) -> Result<Response, AuthError> {
  let user = req.extract_parts_with_state::<User, _>(&state).await?;

  // IMPORTANT: We cannot trust the admin bit in the auth-token, since it may be stale. We need to
  // query the DB.
  if !is_admin(&state, &user.uuid).await {
    return Err(AuthError::Forbidden);
  }

  // CSRF protection.
  let Some(received_csrf_token) = req
    .headers()
    .get(HEADER_CSRF_TOKEN)
    .and_then(|header| header.to_str().ok())
  else {
    return Err(AuthError::BadRequest("admin APIs require csrf header"));
  };

  let expected_csrf = &user.csrf_token;
  if expected_csrf != received_csrf_token {
    return Err(AuthError::BadRequest("invalid CSRF token"));
  }

  return Ok(next.run(req).await);
}

fn cow_to_bytes(cow: Cow<'static, [u8]>) -> Bytes {
  match cow {
    Cow::Borrowed(x) => Bytes::from(x),
    Cow::Owned(x) => Bytes::from(x),
  }
}
