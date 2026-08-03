//! The client side of talking to an external provider: what we authenticate *with*
//! ([`ProviderClient`]) and how we then ask *who the user is* ([`UserApi`]).
//!
//! Both are shared across the provider implementations in this directory, including the ones that
//! are too irregular to be a [`super::social::SocialSpec`].

use oauth2::TokenResponse as _;
use serde::de::DeserializeOwned;
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::providers::OAuthProviderError;
use crate::auth::oauth::providers::interface::{OAuthClientSettings, TokenResponse};
use crate::config::proto::OAuthProviderConfig;

/// Credentials and endpoints of a provider whose URLs are known at compile time.
///
/// Shared by [`super::social::SocialProvider`] and Apple, which otherwise have nothing in common:
/// Apple reads its claims off a JWT rather than a user API, but it still has to pick the same two
/// secrets out of the config and hand back the same settings.
pub struct ProviderClient {
  client_id: String,
  client_secret: String,
  auth_url: Url,
  token_url: Url,
}

impl ProviderClient {
  /// `display_name` only names the provider in the error. The URLs must be parseable.
  pub fn new(
    config: &OAuthProviderConfig,
    display_name: &str,
    auth_url: &str,
    token_url: &str,
  ) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing(format!(
        "{display_name} client id"
      )));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(format!(
        "{display_name} client secret"
      )));
    };

    return Ok(Self {
      client_id,
      client_secret,
      // NOTE: Infallible, callers pass compile-time constants.
      auth_url: Url::parse(auth_url).expect("infallible"),
      token_url: Url::parse(token_url).expect("infallible"),
    });
  }

  pub fn client_id(&self) -> &str {
    return &self.client_id;
  }

  pub fn settings(&self) -> OAuthClientSettings {
    return OAuthClientSettings {
      auth_url: self.auth_url.clone(),
      token_url: self.token_url.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    };
  }
}

/// Bearer-authenticated client for a provider's user-info API.
pub struct UserApi<'a> {
  client: reqwest::Client,
  access_token: &'a str,
  user_api_url: &'a str,
  headers: Vec<(&'static str, String)>,
}

impl<'a> UserApi<'a> {
  /// Constructing through the token response is what keeps the bearer check from being forgotten
  /// by any one provider.
  ///
  /// `headers` are sent on top of the bearer token, for the providers that demand more.
  pub fn new(
    token_response: &'a TokenResponse,
    user_api_url: &'a str,
    headers: Vec<(&'static str, String)>,
  ) -> Result<Self, AuthError> {
    if *token_response.token_type() != oauth2::basic::BasicTokenType::Bearer {
      return Err(AuthError::Internal(
        format!("Unexpected token type: {:?}", token_response.token_type()).into(),
      ));
    }

    return Ok(Self {
      client: reqwest::Client::new(),
      access_token: token_response.access_token().secret(),
      user_api_url,
      headers,
    });
  }

  /// The provider's user-info endpoint.
  ///
  /// Providers making follow-up calls must derive them from this rather than from a constant, so
  /// tests can redirect those too.
  pub fn user_api_url(&self) -> &str {
    return self.user_api_url;
  }

  pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, AuthError> {
    let mut request = self.client.get(url).bearer_auth(self.access_token);
    for (name, value) in &self.headers {
      request = request.header(*name, value);
    }

    return request
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?
      .json::<T>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()));
  }
}
