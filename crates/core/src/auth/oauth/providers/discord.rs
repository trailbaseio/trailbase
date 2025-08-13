use async_trait::async_trait;
use lazy_static::lazy_static;
use oauth2::TokenResponse as _;
use serde::Deserialize;
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::provider::TokenResponse;
use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderFactory};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

pub(crate) struct DiscordOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl DiscordOAuthProvider {
  const NAME: &'static str = "discord";
  const DISPLAY_NAME: &'static str = "Discord";

  const AUTH_URL: &'static str = "https://discord.com/oauth2/authorize";
  const TOKEN_URL: &'static str = "https://discord.com/api/oauth2/token";
  const USER_API_URL: &'static str = "https://discord.com/api/users/@me";

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

  pub fn factory() -> OAuthProviderFactory {
    OAuthProviderFactory {
      id: OAuthProviderId::Discord,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }
}

#[async_trait]
impl OAuthProvider for DiscordOAuthProvider {
  fn name(&self) -> &'static str {
    Self::NAME
  }
  fn provider(&self) -> OAuthProviderId {
    OAuthProviderId::Discord
  }
  fn display_name(&self) -> &'static str {
    Self::DISPLAY_NAME
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    lazy_static! {
      static ref AUTH_URL: Url = Url::parse(DiscordOAuthProvider::AUTH_URL).expect("infallible");
      static ref TOKEN_URL: Url = Url::parse(DiscordOAuthProvider::TOKEN_URL).expect("infallible");
    }

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self) -> Vec<&'static str> {
    return vec!["identify", "email"];
  }

  async fn get_user(&self, token_response: &TokenResponse) -> Result<OAuthUser, AuthError> {
    if *token_response.token_type() != oauth2::basic::BasicTokenType::Bearer {
      return Err(AuthError::Internal(
        format!("Unexpected token type: {:?}", token_response.token_type()).into(),
      ));
    }

    let response = reqwest::Client::new()
      .get(Self::USER_API_URL)
      .bearer_auth(token_response.access_token().secret())
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    //  Checkout available fields on: https://discord.com/developers/docs/resources/user
    #[derive(Default, Deserialize, Debug)]
    struct DiscordUser {
      id: String,
      email: String,
      verified: bool,

      // discriminator: Option<String>,
      // username: Option<String>,
      avatar: Option<String>,
    }

    let user = response
      .json::<DiscordUser>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;
    let verified = user.verified;
    if !verified {
      return Err(AuthError::Unauthorized);
    }

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
      email: user.email,
      verified: user.verified,
      avatar,
    });
  }
}
