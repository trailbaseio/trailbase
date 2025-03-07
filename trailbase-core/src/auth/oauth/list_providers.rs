use axum::extract::State;
use axum::Json;
use serde::Serialize;
use ts_rs::TS;

use crate::auth::AuthError;
use crate::AppState;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ConfiguredOAuthProvidersResponse {
  /// List of tuples (<name>, <display_name>).
  pub providers: Vec<(String, String)>,
}

pub(crate) async fn list_configured_providers_handler(
  State(app_state): State<AppState>,
) -> Result<Json<ConfiguredOAuthProvidersResponse>, AuthError> {
  let providers = app_state.get_oauth_providers();

  return Ok(Json(ConfiguredOAuthProvidersResponse { providers }));
}
