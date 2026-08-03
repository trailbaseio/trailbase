//! External OAuth providers: which ones exist, and the pieces they're built from.
//!
//! - [`interface`]: what a provider must implement, i.e. the [`OAuthProvider`] trait.
//! - [`client`]: how to talk to one, i.e. credentials, endpoints and the user-info request.
//! - [`social`]: the declarative shortcut all but three providers take.
//! - One module per provider, plus the registry at the bottom of this file.

/// What a provider must implement.
pub(crate) mod interface;

/// Credentials, endpoints and the authenticated user-info request.
pub(crate) mod client;

/// Declarative provider descriptions, used by all but Apple, OIDC and the test provider.
mod social;

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
use crate::auth::oauth::providers::social::SocialSpec as _;
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

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
  const N: usize = if cfg!(test) { 11 } else { 10 };
  static REGISTRY: LazyLock<[OAuthProviderFactory; N]> = LazyLock::new(|| {
    [
      #[cfg(test)]
      test::TestOAuthProvider::factory(),
      // NOTE: In the future we might want to have more than one OIDC factory.
      oidc::OidcProvider::factory(0),
      // "Social" OAuth providers.
      //
      // NOTE: All but Apple, which reads its claims off a JWT rather than a user API, are
      // declared as a `social::SocialSpec`.
      apple::AppleOAuthProvider::factory(),
      discord::Discord::factory(),
      gitlab::Gitlab::factory(),
      github::Github::factory(),
      google::Google::factory(),
      facebook::Facebook::factory(),
      microsoft::Microsoft::factory(),
      twitch::Twitch::factory(),
      yandex::Yandex::factory(),
    ]
  });

  return REGISTRY.as_slice();
}
