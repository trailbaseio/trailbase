use async_trait::async_trait;
use oauth2::TokenResponse as _;
use serde::de::DeserializeOwned;
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::provider::{OAuthClientSettings, OAuthProvider, OAuthUser, TokenResponse};
use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderRegistryEntry};
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId, UserIdentifier};

/// This is a wrapper on top of OAuthProvider that should work for most social providers, just to
/// make their implementation a bit more straight forward.
pub trait SimpleOAuthProvider: Send + Sync {
  const ID: OAuthProviderId;
  const NAME: &'static str;
  const DISPLAY_NAME: &'static str;

  // URLs:
  const AUTH_URL: &'static str;
  const TOKEN_URL: &'static str;
  const USER_API_URL: &'static str;

  type User: DeserializeOwned + TryInto<OAuthUser, Error = AuthError>;

  fn client_id(&self) -> String;
  fn client_secret(&self) -> String;
  fn oauth_scopes(&self, user_identifier: UserIdentifier) -> Vec<String>;

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError>
  where
    Self: Sized;
}

pub fn generic_factory<T: SimpleOAuthProvider + Sized + 'static>() -> OAuthProviderRegistryEntry {
  return OAuthProviderRegistryEntry {
    id: T::ID,
    factory_name: T::NAME,
    factory_display_name: T::DISPLAY_NAME,
    factory: Box::new(|_name: &str, config: &OAuthProviderConfig| Ok(Box::new(T::new(config)?))),
  };
}

#[async_trait]
impl<T: SimpleOAuthProvider> OAuthProvider for T {
  fn provider(&self) -> OAuthProviderId {
    return T::ID;
  }

  fn name(&self) -> &str {
    return T::NAME;
  }

  fn display_name(&self) -> &str {
    return T::DISPLAY_NAME;
  }

  fn oauth_scopes(&self, user_identifier: UserIdentifier) -> Vec<String> {
    return (self as &T).oauth_scopes(user_identifier);
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    fn parse(s: &str) -> Result<Url, AuthError> {
      return Url::parse(s)
        .map_err(|_err| AuthError::Internal("invalid OAuth provider configuration".into()));
    }

    return Ok(OAuthClientSettings {
      auth_url: parse(T::AUTH_URL)?,
      token_url: parse(T::TOKEN_URL)?,
      client_id: self.client_id(),
      client_secret: self.client_secret(),
    });
  }

  async fn get_user(
    &self,
    http_client: &reqwest::Client,
    token_response: &TokenResponse,
  ) -> Result<OAuthUser, AuthError> {
    if *token_response.token_type() != oauth2::basic::BasicTokenType::Bearer {
      return Err(AuthError::Internal(
        format!("Unexpected token type: {:?}", token_response.token_type()).into(),
      ));
    }

    let response = http_client
      .get(T::USER_API_URL)
      .bearer_auth(token_response.access_token().secret())
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    let user = response
      .json::<T::User>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    return user.try_into();
  }
}
