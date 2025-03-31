mod discord;
mod facebook;
mod gitlab;
mod google;
mod microsoft;
mod oidc;

#[cfg(test)]
pub(crate) mod test;

use lazy_static::lazy_static;
use std::collections::hash_map::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tracing::*;

use crate::auth::oauth::OAuthProvider;
use crate::config::proto::{AuthConfig, OAuthProviderConfig, OAuthProviderId};

#[derive(Debug, Error)]
pub enum OAuthProviderError {
  #[error("Missing error: {0}")]
  Missing(String),
}

pub type OAuthProviderType = Box<dyn OAuthProvider + Send + Sync>;
type OAuthFactoryType =
  dyn Fn(&str, &OAuthProviderConfig) -> Result<OAuthProviderType, OAuthProviderError> + Send + Sync;

pub(crate) struct OAuthProviderFactory {
  pub id: OAuthProviderId,
  pub factory_name: &'static str,
  pub factory_display_name: &'static str,
  pub factory: Box<OAuthFactoryType>,
}

lazy_static! {
  pub(crate) static ref oauth_provider_registry: Vec<OAuthProviderFactory> = vec![
    #[cfg(test)]
    test::TestOAuthProvider::factory(),
    // NOTE: In the future we might want to have more than one OIDC factory.
    oidc::OidcProvider::factory(0),

    // "Social" OAuth providers.
    discord::DiscordOAuthProvider::factory(),
    gitlab::GitlabOAuthProvider::factory(),
    google::GoogleOAuthProvider::factory(),
    facebook::FacebookOAuthProvider::factory(),
    microsoft::MicrosoftOAuthProvider::factory(),
  ];
}

#[derive(Default)]
pub struct ConfiguredOAuthProviders {
  providers: HashMap<String, Arc<OAuthProviderType>>,
}

impl ConfiguredOAuthProviders {
  pub fn from_config(config: AuthConfig) -> Result<Self, OAuthProviderError> {
    let mut providers = HashMap::<String, Arc<OAuthProviderType>>::new();

    for (key, config) in config.oauth_providers {
      let entry = oauth_provider_registry
        .iter()
        .find(|registered| config.provider_id == Some(registered.id as i32));

      let Some(entry) = entry else {
        return Err(OAuthProviderError::Missing(format!(
          "Missing implementation for oauth provider: {key}"
        )));
      };

      let provider = Arc::new((entry.factory)(&key, &config)?);
      providers.insert(provider.name().to_string(), provider);
    }

    return Ok(ConfiguredOAuthProviders { providers });
  }

  pub fn lookup(&self, name: &str) -> Option<&Arc<OAuthProviderType>> {
    if let Some(entry) = self.providers.get(name) {
      return Some(entry);
    }
    return None;
  }

  pub fn list(&self) -> Vec<(&str, &str)> {
    return self
      .providers
      .values()
      .map(|p| (p.name(), p.display_name()))
      .collect();
  }
}
