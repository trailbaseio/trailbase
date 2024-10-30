use axum::extract::State;
use axum::Json;
use serde::Serialize;
use ts_rs::TS;

use crate::auth::AuthError;
use crate::AppState;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ConfiguredOAuthProvidersResponse {
  pub providers: Vec<(String, String)>,
}

// This handler receives the ?code=<>&state=<>, uses it to get an external oauth token, gets the
// user's information, creates a new local user if needed, and finally mints our own tokens.

pub(crate) async fn list_configured_providers_handler(
  State(app_state): State<AppState>,
) -> Result<Json<ConfiguredOAuthProvidersResponse>, AuthError> {
  let providers = app_state.get_oauth_providers();

  return Ok(Json(ConfiguredOAuthProvidersResponse { providers }));
}
