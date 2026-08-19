use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::provider::{TokenResponse, UserIdentifier};
use crate::auth::oauth::providers::OAuthProviderRegistryEntry;
use crate::auth::oauth::simple_provider::get_user_helper;
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

#[derive(Default, Debug, Deserialize, Serialize)]
pub struct TestUser {
  pub id: String,
  pub email: String,
  pub verified: bool,
}

impl TryFrom<TestUser> for OAuthUser {
  type Error = AuthError;

  fn try_from(user: TestUser) -> Result<Self, Self::Error> {
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

#[derive(Debug)]
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

  pub fn factory() -> OAuthProviderRegistryEntry {
    fn fallback_url(s: &str) -> String {
      return format!("http://auth.org/{s}");
    }

    return OAuthProviderRegistryEntry {
      id: OAuthProviderId::Test,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(TestOAuthProvider {
          client_id: config.client_id.clone().unwrap(),
          client_secret: config.client_secret.clone().unwrap(),
          auth_url: config
            .auth_url
            .clone()
            .unwrap_or_else(|| fallback_url("auth")),
          token_url: config
            .token_url
            .clone()
            .unwrap_or_else(|| fallback_url("token")),
          user_api_url: config
            .user_api_url
            .clone()
            .unwrap_or_else(|| fallback_url("user")),
        }))
      }),
    };
  }
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
      auth_url: Url::parse(&self.auth_url).expect(&format!("GOT: {self:?}")),
      token_url: Url::parse(&self.token_url).expect(&format!("GOT: {self:?}")),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self, _: UserIdentifier) -> Vec<String> {
    return vec![
      "identity".to_string(),
      "email".to_string(),
      "preferences".to_string(),
    ];
  }

  async fn get_user(
    &self,
    http_client: &reqwest::Client,
    token_response: &TokenResponse,
  ) -> Result<OAuthUser, AuthError> {
    return get_user_helper::<TestUser>(http_client, &self.user_api_url, token_response).await;
  }
}
