use log::*;
use std::collections::HashMap;

use crate::auth::oauth::providers::{OAuthProviderType, build_oauth_providers_from_config};
use crate::auth::password::PasswordOptions;
use crate::config::proto::AuthConfig;

#[derive(Default)]
pub struct AuthOptions {
  password_options: PasswordOptions,
  oauth_providers: HashMap<String, OAuthProviderType>,
}

impl AuthOptions {
  pub fn from_config(config: AuthConfig) -> Self {
    return Self {
      password_options: PasswordOptions::default(),
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
  pub fn list_oauth_providers(&self) -> Vec<(String, String)> {
    return self
      .oauth_providers
      .values()
      .map(|p| (p.name().to_string(), p.display_name().to_string()))
      .collect();
  }
}
