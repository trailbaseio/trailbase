use async_trait::async_trait;
use oauth2::TokenResponse as _;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::provider::TokenResponse;
use crate::auth::oauth::providers::OAuthProviderFactory;
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

pub struct TestOAuthProvider {
  client_id: String,
  client_secret: String,

  auth_url: String,
  token_url: String,
  user_api_url: String,
}

impl TestOAuthProvider {
  pub const NAME: &'static str = "test";
  pub const DISPLAY_NAME: &'static str = "Test OAuth";

  pub fn factory() -> OAuthProviderFactory {
    OAuthProviderFactory {
      id: OAuthProviderId::Test,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(TestOAuthProvider {
          client_id: config.client_id.clone().unwrap(),
          client_secret: config.client_secret.clone().unwrap(),
          auth_url: config.auth_url.clone().unwrap_or("not set".to_string()),
          token_url: config.token_url.clone().unwrap_or("not set".to_string()),
          user_api_url: config.user_api_url.clone().unwrap_or("not set".to_string()),
        }))
      }),
    }
  }
}

#[derive(Default, Debug, Deserialize, Serialize)]
pub struct TestUser {
  pub id: String,
  pub email: String,
  pub verified: bool,
}

#[async_trait]
impl OAuthProvider for TestOAuthProvider {
  fn name(&self) -> &'static str {
    Self::NAME
  }
  fn provider(&self) -> OAuthProviderId {
    OAuthProviderId::Test
  }
  fn display_name(&self) -> &'static str {
    Self::DISPLAY_NAME
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    return Ok(OAuthClientSettings {
      auth_url: Url::parse(&self.auth_url).unwrap(),
      token_url: Url::parse(&self.token_url).unwrap(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self) -> Vec<&'static str> {
    return vec!["identity", "email", "preferences"];
  }

  async fn get_user(&self, token_response: &TokenResponse) -> Result<OAuthUser, AuthError> {
    if *token_response.token_type() != oauth2::basic::BasicTokenType::Bearer {
      return Err(AuthError::Internal(
        format!("Unexpected token type: {:?}", token_response.token_type()).into(),
      ));
    }

    let response = reqwest::Client::new()
      .get(&self.user_api_url)
      .bearer_auth(token_response.access_token().secret())
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    let user = response
      .json::<TestUser>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: OAuthProviderId::Test,
      email: user.email,
      verified: user.verified,
      avatar: None,
    });
  }
}
