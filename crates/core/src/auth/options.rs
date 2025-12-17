use log::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::auth::oauth::providers::{OAuthProviderType, build_oauth_providers_from_config};
use crate::auth::password::PasswordOptions;
use crate::config::proto::AuthConfig;

#[derive(Clone, Default)]
pub(crate) struct AuthOptions {
  password_options: PasswordOptions,
  oauth_providers: HashMap<String, Arc<OAuthProviderType>>,

  pub has_login_ui: bool,
  pub has_register_ui: bool,
  pub has_profile_ui: bool,
}

impl PartialEq for AuthOptions {
  fn eq(&self, other: &Self) -> bool {
    let p0: HashSet<&String> = self.oauth_providers.keys().collect();
    let p1: HashSet<&String> = other.oauth_providers.keys().collect();

    return p0 == p1
      && self.password_options == other.password_options
      && self.has_login_ui == other.has_login_ui
      && self.has_register_ui == other.has_register_ui
      && self.has_profile_ui == other.has_profile_ui;
  }
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
      has_login_ui: false,
      has_register_ui: false,
      has_profile_ui: false,
    };
  }

  pub fn password_options(&self) -> &PasswordOptions {
    return &self.password_options;
  }

  pub fn lookup_oauth_provider(&self, name: &str) -> Option<&Arc<OAuthProviderType>> {
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
