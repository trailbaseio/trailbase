use async_trait::async_trait;
use std::sync::LazyLock;
use url::Url;

use crate::auth::AuthError;
use crate::auth::apple::{decode_id_token_with_keys, fetch_apple_public_keys};
use crate::auth::oauth::provider::TokenResponse;
use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderRegistryEntry};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto;

pub(crate) struct AppleOAuthProvider {
  client_id: String,
  client_secret: String,
}

/// Apple OAuth2 provider, also known as "Sign-in with Apple".
impl AppleOAuthProvider {
  const NAME: &'static str = "apple";
  const DISPLAY_NAME: &'static str = "Apple";

  fn new(config: &proto::OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing("Apple client id".to_string()));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Apple client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }

  pub fn registry_entry() -> OAuthProviderRegistryEntry {
    OAuthProviderRegistryEntry {
      id: proto::OAuthProviderId::Apple,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &proto::OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }
}

#[async_trait]
impl OAuthProvider for AppleOAuthProvider {
  fn name(&self) -> &'static str {
    return Self::NAME;
  }

  fn auth_type(&self) -> oauth2::AuthType {
    // Apple only accepts client credentials in the POST body, not via HTTP Basic auth:
    // https://developer.apple.com/documentation/signinwithapple/request_and_validate_tokens
    return oauth2::AuthType::RequestBody;
  }

  fn provider(&self) -> proto::OAuthProviderId {
    return proto::OAuthProviderId::Apple;
  }

  fn display_name(&self) -> &'static str {
    return Self::DISPLAY_NAME;
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    static AUTH_URL: LazyLock<Url> = LazyLock::new(|| {
      // When scopes "name" and/or "email" are specified, apple expects `response_mode=form_post`
      // and to call-back using a POST method:
      //   https://developer.apple.com/documentation/signinwithapple/incorporating-sign-in-with-apple-into-other-platforms
      const AUTH_URL: &str = "https://appleid.apple.com/auth/authorize?response_mode=form_post";
      return Url::parse(AUTH_URL).expect("tested");
    });
    static TOKEN_URL: LazyLock<Url> = LazyLock::new(|| {
      const TOKEN_URL: &str = "https://appleid.apple.com/auth/token";
      return Url::parse(TOKEN_URL).expect("tested");
    });

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self, _: proto::UserIdentifier) -> Vec<String> {
    // TODO: Pick scopes based on user-id policy.
    return vec!["name".to_string(), "email".to_string()];
  }

  /// Unlike most other OAuth provider, Apple doesn't have a user api, but rather puts claims in
  /// the JWT id_token.
  async fn get_user(
    &self,
    http_client: &reqwest::Client,
    token_response: &TokenResponse,
  ) -> Result<OAuthUser, AuthError> {
    let Some(ref id_token) = token_response.extra_fields().id_token else {
      return Err(AuthError::BadRequest("missing id token"));
    };

    let public_keys = fetch_apple_public_keys(http_client).await?;
    let apple_id_token = decode_id_token_with_keys(&public_keys, id_token, &self.client_id)?;

    let Some(email) = apple_id_token.email else {
      return Err(AuthError::BadRequest("missing email"));
    };

    return Ok(OAuthUser {
      provider_user_id: apple_id_token.sub,
      provider_id: proto::OAuthProviderId::Apple,
      email: Some(email),
      username: None,
      verified: apple_id_token.email_verified.is_some_and(|v| v.value()),
      avatar: None,
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_apple_settings() {
    let provider = AppleOAuthProvider {
      client_id: "12345".to_string(),
      client_secret: "s3cre7".to_string(),
    };

    let settings = provider.settings().unwrap();
    let query: Vec<_> = settings.auth_url.query_pairs().collect();
    assert!(!query.is_empty());
  }

  #[test]
  fn test_apple_auth_type_is_request_body() {
    // Apple only accepts client credentials in the POST body, not HTTP Basic auth.
    let provider = AppleOAuthProvider {
      client_id: "12345".to_string(),
      client_secret: "s3cre7".to_string(),
    };

    assert!(matches!(
      provider.auth_type(),
      oauth2::AuthType::RequestBody
    ));
  }
}
