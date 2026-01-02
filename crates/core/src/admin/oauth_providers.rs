use axum::extract::Json;
use serde::Serialize;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::auth::oauth::providers::oauth_providers_static_registry;

#[derive(Debug, Serialize, ts_rs::TS)]
pub struct OAuthProviderEntry {
  pub id: i32,
  pub name: String,
  pub display_name: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct OAuthProviderResponse {
  providers: Vec<OAuthProviderEntry>,
}

/// Lists all possible providers. This is different from the public /api/v1/auth/oauth/providers,
/// which only lists configured providers.
pub async fn available_oauth_providers_handler() -> Result<Json<OAuthProviderResponse>, Error> {
  return Ok(Json(OAuthProviderResponse {
    providers: oauth_providers_static_registry()
      .iter()
      .map(|factory| OAuthProviderEntry {
        id: factory.id as i32,
        name: factory.factory_name.to_string(),
        display_name: factory.factory_display_name.to_string(),
      })
      .collect(),
  }));
}
