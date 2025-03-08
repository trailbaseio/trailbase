use async_trait::async_trait;
use lazy_static::lazy_static;
use serde::Deserialize;
use url::Url;

use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderFactory};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::auth::AuthError;
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

#[derive(Default, Deserialize, Debug)]
struct FacebookUserPictureData {
  url: String,
}

#[derive(Default, Deserialize, Debug)]
struct FacebookUserPicture {
  data: FacebookUserPictureData,
}

#[derive(Default, Deserialize, Debug)]
struct FacebookUser {
  id: String,
  email: String,
  // name: Option<String>,
  picture: Option<FacebookUserPicture>,
}

pub(crate) struct FacebookOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl FacebookOAuthProvider {
  const NAME: &'static str = "facebook";
  const DISPLAY_NAME: &'static str = "Facebook";

  const AUTH_URL: &'static str = "https://www.facebook.com/v3.2/dialog/oauth";
  const TOKEN_URL: &'static str = "https://graph.facebook.com/v3.2/oauth/access_token";
  const USER_API_URL: &'static str =
    "https://graph.facebook.com/me?fields=name,email,picture.type(large)";

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing(
        "Facebook client id".to_string(),
      ));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Facebook client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }

  pub fn factory() -> OAuthProviderFactory {
    OAuthProviderFactory {
      id: OAuthProviderId::Facebook,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }
}

#[async_trait]
impl OAuthProvider for FacebookOAuthProvider {
  fn name(&self) -> &'static str {
    Self::NAME
  }
  fn provider(&self) -> OAuthProviderId {
    OAuthProviderId::Facebook
  }
  fn display_name(&self) -> &'static str {
    Self::DISPLAY_NAME
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    lazy_static! {
      static ref AUTH_URL: Url = Url::parse(FacebookOAuthProvider::AUTH_URL).expect("infallible");
      static ref TOKEN_URL: Url = Url::parse(FacebookOAuthProvider::TOKEN_URL).expect("infallible");
    }

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self) -> Vec<&'static str> {
    return vec!["email"];
  }

  async fn get_user(&self, access_token: String) -> Result<OAuthUser, AuthError> {
    let response = reqwest::Client::new()
      .get(Self::USER_API_URL)
      .bearer_auth(access_token)
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    let user = response
      .json::<FacebookUser>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: OAuthProviderId::Facebook,
      email: user.email,
      verified: true,
      avatar: user.picture.map(|p| p.data.url),
    });
  }
}
