use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::providers::OAuthProviderFactory;
use crate::auth::oauth::providers::client::UserApi;
use crate::auth::oauth::providers::interface::TokenResponse;
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
  fn name(&self) -> &str {
    return Self::NAME;
  }
  fn display_name(&self) -> &str {
    return Self::DISPLAY_NAME;
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    return Ok(OAuthClientSettings {
      auth_url: Url::parse(&self.auth_url).unwrap(),
      token_url: Url::parse(&self.token_url).unwrap(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self) -> Vec<&str> {
    return vec!["identity", "email", "preferences"];
  }

  async fn get_user(&self, token_response: &TokenResponse) -> Result<OAuthUser, AuthError> {
    let api = UserApi::new(token_response, &self.user_api_url, vec![])?;
    let user = api.get_json::<TestUser>(api.user_api_url()).await?;

    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: OAuthProviderId::Test,
      email: Some(user.email),
      username: None,
      verified: user.verified,
      avatar: None,
    });
  }
}
