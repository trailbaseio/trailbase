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
use crate::auth::oauth::simple_provider::SimpleOAuthProvider;
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
      oidc::OidcProvider::registry_entry(0),
      // "Social" OAuth providers.
      apple::AppleOAuthProvider::registry_entry(),
      simple_provider_registry_entry::<discord::DiscordOAuthProvider>(),
      simple_provider_registry_entry::<gitlab::GitlabOAuthProvider>(),
      github::GithubOAuthProvider::registry_entry(),
      simple_provider_registry_entry::<google::GoogleOAuthProvider>(),
      simple_provider_registry_entry::<facebook::FacebookOAuthProvider>(),
      simple_provider_registry_entry::<microsoft::MicrosoftOAuthProvider>(),
      twitch::TwitchOAuthProvider::registry_entry(),
      simple_provider_registry_entry::<yandex::YandexOAuthProvider>(),
    ]
  });

  return REGISTRY.as_slice();
}

fn simple_provider_registry_entry<T: SimpleOAuthProvider + Sized + 'static>()
-> OAuthProviderRegistryEntry {
  #[cfg(test)]
  url::Url::parse(T::USER_API_URL).expect("test-only");

  return OAuthProviderRegistryEntry {
    id: T::ID,
    factory_name: T::NAME,
    factory_display_name: T::DISPLAY_NAME,
    factory: Box::new(|name: &str, config: &OAuthProviderConfig| {
      debug_assert_eq!(T::NAME, name);

      return Ok(Box::new(T::new(config)?));
    }),
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_registry() {
    let config = OAuthProviderConfig {
      client_id: Some("id".to_string()),
      client_secret: Some("secret".to_string()),
      // Below URLs will be ignored by the SocialProviders.
      auth_url: Some("http://auth.org/auth".to_string()),
      token_url: Some("http://auth.org/token".to_string()),
      user_api_url: Some("http://auth.org/user".to_string()),
      ..Default::default()
    };

    for entry in oauth_providers_static_registry() {
      let provider = (*entry.factory)(&entry.factory_name, &config).unwrap();

      assert_eq!(entry.id, provider.provider());
      assert_eq!(entry.factory_name, provider.name());

      // Make sure URLs parse.
      provider.settings().unwrap();
    }
  }
}
