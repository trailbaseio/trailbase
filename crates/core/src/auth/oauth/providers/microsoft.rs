use async_trait::async_trait;
use lazy_static::lazy_static;
use serde::Deserialize;
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderFactory};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

#[derive(Default, Deserialize, Debug)]
struct MicrosoftUser {
  id: String,
  mail: String,
}

pub(crate) struct MicrosoftOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl MicrosoftOAuthProvider {
  const NAME: &'static str = "microsoft";
  const DISPLAY_NAME: &'static str = "Microsoft";

  const AUTH_URL: &'static str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
  const TOKEN_URL: &'static str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
  const USER_API_URL: &'static str = "https://graph.microsoft.com/v1.0/me";

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing(
        "Microsoft client id".to_string(),
      ));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Microsoft client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }

  pub fn factory() -> OAuthProviderFactory {
    OAuthProviderFactory {
      id: OAuthProviderId::Microsoft,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }
}

#[async_trait]
impl OAuthProvider for MicrosoftOAuthProvider {
  fn name(&self) -> &'static str {
    Self::NAME
  }
  fn provider(&self) -> OAuthProviderId {
    OAuthProviderId::Microsoft
  }
  fn display_name(&self) -> &'static str {
    Self::DISPLAY_NAME
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    lazy_static! {
      static ref AUTH_URL: Url = Url::parse(MicrosoftOAuthProvider::AUTH_URL).expect("infallible");
      static ref TOKEN_URL: Url =
        Url::parse(MicrosoftOAuthProvider::TOKEN_URL).expect("infallible");
    }

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self) -> Vec<&'static str> {
    return vec!["User.Read"];
  }

  async fn get_user(&self, access_token: String) -> Result<OAuthUser, AuthError> {
    let response = reqwest::Client::new()
      .get(Self::USER_API_URL)
      .bearer_auth(access_token)
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    let user = response
      .json::<MicrosoftUser>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: OAuthProviderId::Microsoft,
      email: user.mail,
      verified: true,
      avatar: None,
    });
  }
}
