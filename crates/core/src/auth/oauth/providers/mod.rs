mod apple;
mod discord;
mod facebook;
mod gitlab;
mod google;
mod microsoft;
mod oidc;
mod twitch;

#[cfg(test)]
pub(crate) mod test;

use std::collections::hash_map::HashMap;
use std::sync::LazyLock;
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
  dyn Fn(&str, &OAuthProviderConfig) -> Result<OAuthProviderType, OAuthProviderError> + Send + Sync;

pub(crate) struct OAuthProviderFactory {
  pub id: OAuthProviderId,
  pub factory_name: &'static str,
  pub factory_display_name: &'static str,
  pub factory: Box<OAuthFactoryType>,
}

pub(crate) fn oauth_providers_static_registry() -> &'static [OAuthProviderFactory] {
  const N: usize = if cfg!(test) { 9 } else { 8 };
  static REGISTRY: LazyLock<[OAuthProviderFactory; N]> = LazyLock::new(|| {
    [
      #[cfg(test)]
      test::TestOAuthProvider::factory(),
      // NOTE: In the future we might want to have more than one OIDC factory.
      oidc::OidcProvider::factory(0),
      // "Social" OAuth providers.
      apple::AppleOAuthProvider::factory(),
      discord::DiscordOAuthProvider::factory(),
      gitlab::GitlabOAuthProvider::factory(),
      google::GoogleOAuthProvider::factory(),
      facebook::FacebookOAuthProvider::factory(),
      microsoft::MicrosoftOAuthProvider::factory(),
      twitch::TwitchOAuthProvider::factory(),
    ]
  });

  return REGISTRY.as_slice();
}

pub(crate) fn build_oauth_providers_from_config(
  config: AuthConfig,
) -> Result<HashMap<String, OAuthProviderType>, OAuthProviderError> {
  return config
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
      return Ok((provider.name().to_string(), provider));
    })
    .collect();
}
