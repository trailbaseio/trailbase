mod apple;
mod discord;
mod facebook;
mod github;
mod gitlab;
mod google;
mod microsoft;
mod oidc;
mod twitch;
mod yandex;

#[cfg(test)]
pub(crate) mod test;

use std::sync::LazyLock;
use thiserror::Error;

use crate::auth::oauth::OAuthProvider;
use crate::auth::oauth::simple_provider::generic_factory;
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

#[derive(Debug, Error)]
pub enum OAuthProviderError {
  #[error("Missing: {0}")]
  Missing(String),
}

type OAuthProviderType = Box<dyn OAuthProvider + Send + Sync>;

type OAuthFactoryType =
  dyn Fn(&str, &OAuthProviderConfig) -> Result<OAuthProviderType, OAuthProviderError> + Send + Sync;

pub(crate) struct OAuthProviderRegistryEntry {
  pub id: OAuthProviderId,
  pub factory_name: &'static str,
  pub factory_display_name: &'static str,
  pub factory: Box<OAuthFactoryType>,
}

pub(crate) fn oauth_providers_static_registry() -> &'static [OAuthProviderRegistryEntry] {
  const N: usize = if cfg!(test) { 11 } else { 10 };
  static REGISTRY: LazyLock<[OAuthProviderRegistryEntry; N]> = LazyLock::new(|| {
    [
      #[cfg(test)]
      test::TestOAuthProvider::factory(),
      // NOTE: In the future we might want to have more than one OIDC factory.
      oidc::OidcProvider::factory(0),
      // "Social" OAuth providers.
      apple::AppleOAuthProvider::factory(),
      generic_factory::<discord::DiscordOAuthProvider>(),
      generic_factory::<gitlab::GitlabOAuthProvider>(),
      github::GithubOAuthProvider::factory(),
      google::GoogleOAuthProvider::factory(),
      generic_factory::<facebook::FacebookOAuthProvider>(),
      generic_factory::<microsoft::MicrosoftOAuthProvider>(),
      twitch::TwitchOAuthProvider::factory(),
      yandex::YandexOAuthProvider::factory(),
    ]
  });

  return REGISTRY.as_slice();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_registry() {
    let registry = oauth_providers_static_registry();

    let config = OAuthProviderConfig {
      client_id: Some("id".to_string()),
      client_secret: Some("secret".to_string()),
      auth_url: Some("http://auth.org/".to_string()),
      user_api_url: Some("http://auth.org/".to_string()),
      token_url: Some("http://auth.org/".to_string()),
      ..Default::default()
    };

    for entry in registry {
      let provider = (*entry.factory)(&entry.factory_name, &config).unwrap();

      assert_eq!(entry.id, provider.provider());
      assert_eq!(entry.factory_name, provider.name());
    }
  }
}
