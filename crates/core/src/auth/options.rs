use indexmap::IndexMap;
use itertools::Itertools;
use log::*;

use crate::auth::oauth::providers::{
  OAuthProviderError, OAuthProviderType, oauth_providers_static_registry,
};
use crate::auth::password::PasswordOptions;
use crate::config::proto::AuthConfig;

#[derive(Default)]
pub struct AuthOptions {
  password_options: PasswordOptions,
  oauth_providers: IndexMap<String, OAuthProviderType>,
}

#[derive(Default)]
pub struct OAuthProvider {
  pub name: String,
  pub display_name: String,
}

impl AuthOptions {
  pub fn from_config(config: AuthConfig) -> Self {
    return Self {
      password_options: PasswordOptions {
        min_length: config.password_minimal_length.unwrap_or(8) as usize,
        max_length: 128,
        must_contain_upper_and_lower_case: config
          .password_must_contain_upper_and_lower_case
          .unwrap_or(false),
        must_contain_digits: config.password_must_contain_digits.unwrap_or(false),
        must_contain_special_characters: config
          .password_must_contain_special_characters
          .unwrap_or(false),
      },
      oauth_providers: build_oauth_providers_from_config(config).unwrap_or_else(|err| {
        error!("Failed to derive configured OAuth providers from config: {err}");
        return Default::default();
      }),
    };
  }

  pub fn password_options(&self) -> &PasswordOptions {
    return &self.password_options;
  }

  pub fn lookup_oauth_provider(&self, name: &str) -> Option<&OAuthProviderType> {
    if let Some(entry) = self.oauth_providers.get(name) {
      return Some(entry);
    }
    return None;
  }

  /// Returns list of tuples with (name, display_name);
  pub fn list_oauth_providers(&self) -> Vec<OAuthProvider> {
    return self
      .oauth_providers
      .values()
      .map(|p| OAuthProvider {
        name: p.name().to_string(),
        display_name: p.display_name().to_string(),
      })
      .collect();
  }
}

fn build_oauth_providers_from_config(
  config: AuthConfig,
) -> Result<IndexMap<String, OAuthProviderType>, OAuthProviderError> {
  let providers = config
    .oauth_providers
    .iter()
    .map(|(key, config)| {
      let entry = oauth_providers_static_registry()
        .iter()
        .find(|registered| config.provider_id == Some(registered.id as i32));

      let Some(entry) = entry else {
        return Err(OAuthProviderError::Missing(format!(
          "Missing implementation for oauth provider: {key}"
        )));
      };

      let provider = (entry.factory)(key, config)?;
      return Ok(provider);
    })
    .collect::<Result<Vec<_>, _>>()?;

  return Ok(IndexMap::from_iter(
    providers
      .into_iter()
      .sorted_by(|a, b| Ord::cmp(a.name(), b.name()))
      .map(|p| (p.name().to_string(), p)),
  ));
}
