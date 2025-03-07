use async_trait::async_trait;
use lazy_static::lazy_static;
use serde::Deserialize;
use url::Url;

use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderFactory};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::auth::AuthError;
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

pub(crate) struct GoogleOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl GoogleOAuthProvider {
  const NAME: &'static str = "google";
  const DISPLAY_NAME: &'static str = "Google";

  const AUTH_URL: &'static str = "https://accounts.google.com/o/oauth2/auth";
  const TOKEN_URL: &'static str = "https://accounts.google.com/o/oauth2/token";
  const USER_API_URL: &'static str = "https://www.googleapis.com/oauth2/v1/userinfo";

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing("Google client id".to_string()));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Google client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }

  pub fn factory() -> OAuthProviderFactory {
    OAuthProviderFactory {
      id: OAuthProviderId::Google,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }
}

#[async_trait]
impl OAuthProvider for GoogleOAuthProvider {
  fn name(&self) -> &'static str {
    Self::NAME
  }
  fn provider(&self) -> OAuthProviderId {
    OAuthProviderId::Google
  }
  fn display_name(&self) -> &'static str {
    Self::DISPLAY_NAME
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    lazy_static! {
      static ref AUTH_URL: Url = Url::parse(GoogleOAuthProvider::AUTH_URL).unwrap();
      static ref TOKEN_URL: Url = Url::parse(GoogleOAuthProvider::TOKEN_URL).unwrap();
    }

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self) -> Vec<&'static str> {
    return vec![
      "https://www.googleapis.com/auth/userinfo.profile",
      "https://www.googleapis.com/auth/userinfo.email",
    ];
  }

  async fn get_user(&self, access_token: String) -> Result<OAuthUser, AuthError> {
    let response = reqwest::Client::new()
      .get(Self::USER_API_URL)
      .bearer_auth(access_token)
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    #[derive(Default, Deserialize, Debug)]
    struct GoogleUser {
      id: String,
      // name: Option<String>,
      email: String,
      verified_email: bool,
      picture: Option<String>,
    }

    let user = response
      .json::<GoogleUser>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;
    if !user.verified_email {
      return Err(AuthError::Unauthorized);
    }

    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: OAuthProviderId::Google,
      email: user.email,
      verified: user.verified_email,
      avatar: user.picture,
    });
  }
}
