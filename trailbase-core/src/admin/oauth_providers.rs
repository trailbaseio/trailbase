use axum::extract::Json;
use serde::Serialize;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::auth::oauth::providers::{oauth_provider_registry, OAuthProviderEntry};

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct OAuthProviderResponse {
  providers: Vec<OAuthProviderEntry>,
}

pub async fn available_oauth_providers_handler() -> Result<Json<OAuthProviderResponse>, Error> {
  return Ok(Json(OAuthProviderResponse {
    providers: oauth_provider_registry
      .iter()
      .map(|factory| factory.into())
      .collect(),
  }));
}
