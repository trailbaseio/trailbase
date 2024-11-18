mod discord;
mod facebook;
mod gitlab;
mod google;
mod microsoft;

#[cfg(test)]
pub(crate) mod test;

use lazy_static::lazy_static;
use log::*;
use serde::Serialize;
use std::collections::hash_map::HashMap;
use std::sync::Arc;
use thiserror::Error;

use crate::auth::oauth::OAuthProvider;
use crate::config::proto::{AuthConfig, OAuthProviderConfig, OAuthProviderId};

#[derive(Debug, Error)]
pub enum OAuthProviderError {
  #[error("Missing error: {0}")]
  Missing(String),
}

pub type OAuthProviderType = Box<dyn OAuthProvider + Send + Sync>;
type OAuthFactoryType =
  dyn Fn(&OAuthProviderConfig) -> Result<OAuthProviderType, OAuthProviderError> + Send + Sync;

pub(crate) struct OAuthProviderFactory {
  pub id: OAuthProviderId,
  pub name: &'static str,
  pub display_name: &'static str,
  pub factory: Box<OAuthFactoryType>,
}

#[derive(Debug, Serialize, ts_rs::TS)]
pub struct OAuthProviderEntry {
  pub id: i32,
  pub name: String,
  pub display_name: String,
}

impl From<&OAuthProviderFactory> for OAuthProviderEntry {
  fn from(val: &OAuthProviderFactory) -> Self {
    OAuthProviderEntry {
      id: val.id as i32,
      name: val.name.to_string(),
      display_name: val.display_name.to_string(),
    }
  }
}

lazy_static! {
  pub(crate) static ref oauth_provider_registry: Vec<OAuthProviderFactory> = vec![
    discord::DiscordOAuthProvider::factory(),
    gitlab::GitlabOAuthProvider::factory(),
    google::GoogleOAuthProvider::factory(),
    facebook::FacebookOAuthProvider::factory(),
    microsoft::MicrosoftOAuthProvider::factory(),
    #[cfg(test)]
    test::TestOAuthProvider::factory(),
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

      providers.insert(entry.name.to_string(), (entry.factory)(&config)?.into());
    }

    return Ok(ConfiguredOAuthProviders { providers });
  }

  pub fn lookup(&self, name: &str) -> Option<&Arc<OAuthProviderType>> {
    if let Some(entry) = self.providers.get(name) {
      return Some(entry);
    }
    return None;
  }

  pub fn list(&self) -> Vec<(&'static str, &'static str)> {
    return self
      .providers
      .values()
      .map(|p| (p.name(), p.display_name()))
      .collect();
  }
}
