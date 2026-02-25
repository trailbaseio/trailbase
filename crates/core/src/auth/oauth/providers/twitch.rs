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

pub(crate) struct TwitchOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl TwitchOAuthProvider {
  const NAME: &'static str = "twitch";
  const DISPLAY_NAME: &'static str = "Twitch";

  const AUTH_URL: &'static str = "https://id.twitch.tv/oauth2/authorize";
  const TOKEN_URL: &'static str = "https://id.twitch.tv/oauth2/token";
  const USER_API_URL: &'static str = "https://api.twitch.tv/helix/users";

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing("Twitch client id".to_string()));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Twitch client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }

  pub fn factory() -> OAuthProviderFactory {
    OAuthProviderFactory {
      id: OAuthProviderId::Twitch,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }
}

#[async_trait]
impl OAuthProvider for TwitchOAuthProvider {
  fn name(&self) -> &'static str {
    Self::NAME
  }
  fn provider(&self) -> OAuthProviderId {
    OAuthProviderId::Twitch
  }
  fn display_name(&self) -> &'static str {
    Self::DISPLAY_NAME
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    lazy_static! {
      static ref AUTH_URL: Url = Url::parse(TwitchOAuthProvider::AUTH_URL).expect("infallible");
      static ref TOKEN_URL: Url = Url::parse(TwitchOAuthProvider::TOKEN_URL).expect("infallible");
    }

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self) -> Vec<&'static str> {
    return vec!["user:read:email"];
  }

  async fn get_user(&self, token_response: &TokenResponse) -> Result<OAuthUser, AuthError> {
    if *token_response.token_type() != oauth2::basic::BasicTokenType::Bearer {
      return Err(AuthError::Internal(
        format!("Unexpected token type: {:?}", token_response.token_type()).into(),
      ));
    }

    let response = reqwest::Client::new()
      .get(Self::USER_API_URL)
      .header("Client-Id", &self.client_id)
      .bearer_auth(token_response.access_token().secret())
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    // Reference: https://dev.twitch.tv/docs/api/reference#get-users
    #[derive(Default, Deserialize, Debug)]
    struct TwitchUser {
      id: String,
      // According to reference above, email is implicitly verified.
      email: String,
      // login: String,
      // display_name: String,
      profile_image_url: Option<String>,
    }

    let user = response
      .json::<TwitchUser>()
      .await
      .map_err(|err| AuthError::Internal(err.into()))?;

    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: OAuthProviderId::Twitch,
      email: user.email,
      verified: true,
      avatar: user.profile_image_url,
    });
  }
}
