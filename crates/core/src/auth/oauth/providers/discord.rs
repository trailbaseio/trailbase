use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::OAuthUser;
use crate::auth::oauth::provider::UserIdentifier;
use crate::auth::oauth::providers::OAuthProviderError;
use crate::auth::oauth::simple_provider::SimpleOAuthProvider;
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

// Checkout available fields on: https://discord.com/developers/docs/resources/user
#[derive(Deserialize, Debug)]
pub struct DiscordUser {
  id: String,
  email: String,
  verified: bool,

  // discriminator: Option<String>,
  username: Option<String>,
  avatar: Option<String>,
}

impl TryFrom<DiscordUser> for OAuthUser {
  type Error = AuthError;

  fn try_from(user: DiscordUser) -> Result<Self, Self::Error> {
    // let username = match (user.discriminator, user.username) {
    //   (Some(discriminator), Some(username)) => Some(format!("{username}#{discriminator}")),
    //   (None, Some(username)) => Some(username.to_string()),
    //   (Some(discriminator), None) => Some(discriminator.to_string()),
    //   (None, None) => None,
    // };
    let avatar = user.avatar.map(|avatar| {
      format!(
        "https://cdn.discordapp.com/avatars/{id}/{avatar}.png",
        id = user.id
      )
    });

    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: OAuthProviderId::Discord,
      email: Some(user.email),
      username: user.username,
      verified: user.verified,
      avatar,
    });
  }
}

pub(crate) struct DiscordOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl SimpleOAuthProvider for DiscordOAuthProvider {
  const ID: OAuthProviderId = OAuthProviderId::Discord;
  const NAME: &'static str = "discord";
  const DISPLAY_NAME: &'static str = "Discord";

  const AUTH_URL: &'static str = "https://discord.com/oauth2/authorize";
  const TOKEN_URL: &'static str = "https://discord.com/api/oauth2/token";
  const USER_API_URL: &'static str = "https://discord.com/api/users/@me";

  type User = DiscordUser;

  fn client_id(&self) -> String {
    return self.client_id.clone();
  }

  fn client_secret(&self) -> String {
    return self.client_secret.clone();
  }

  fn oauth_scopes(&self, _: UserIdentifier) -> Vec<String> {
    // TODO: Pick scopes based on user-id policy.
    return vec!["identify".to_string(), "email".to_string()];
  }

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing("Discord client id".to_string()));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Discord client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }
}
