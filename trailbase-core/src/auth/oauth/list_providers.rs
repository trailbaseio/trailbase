use axum::Json;
use axum::extract::State;
use serde::Serialize;
use ts_rs::TS;

use crate::AppState;
use crate::auth::AuthError;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ConfiguredOAuthProvidersResponse {
  /// List of tuples (<name>, <display_name>).
  pub providers: Vec<(String, String)>,
}

pub(crate) async fn list_configured_providers_handler(
  State(app_state): State<AppState>,
) -> Result<Json<ConfiguredOAuthProvidersResponse>, AuthError> {
  let auth_options = app_state.auth_options();

  return Ok(Json(ConfiguredOAuthProvidersResponse {
    providers: auth_options.list_oauth_providers(),
  }));
}
