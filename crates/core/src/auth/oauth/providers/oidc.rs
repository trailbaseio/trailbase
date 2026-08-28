use async_trait::async_trait;
use oauth2::TokenResponse as _;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::provider::TokenResponse;
use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderRegistryEntry};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto;

// TODO: Add name/display name and this would already be a generic CustomOAuthProvider.
pub struct OidcProvider {
  name: String,
  display_name: String,
  client_id: String,
  client_secret: String,

  auth_url: String,
  token_url: String,
  user_api_url: String,
  scopes: Option<Vec<String>>,
}

impl OidcProvider {
  pub fn registry_entry(index: u64) -> OAuthProviderRegistryEntry {
    let (id, factory_name, factory_display_name) = match index {
      0 => (proto::OAuthProviderId::Oidc0, "oidc0", "OpenID Connect"),
      _ => panic!("Multiple OIDC provider not implemented"),
    };

    OAuthProviderRegistryEntry {
      id,
      factory_name,
      factory_display_name,
      factory: Box::new(|name: &str, config: &proto::OAuthProviderConfig| {
        // NOTE: Below errors should not trigger, since already checked by config validation.
        let Some(auth_url) = config.auth_url.clone() else {
          return Err(OAuthProviderError::Missing("Auth url missing".into()));
        };
        let Some(token_url) = config.token_url.clone() else {
          return Err(OAuthProviderError::Missing("Token url missing".into()));
        };
        let Some(user_api_url) = config.user_api_url.clone() else {
          return Err(OAuthProviderError::Missing("User-API url missing".into()));
        };

        Ok(Box::new(OidcProvider {
          name: name.to_string(),
          display_name: config
            .display_name
            .as_deref()
            .unwrap_or(factory_display_name)
            .to_string(),
          client_id: config.client_id.clone().expect("startup"),
          client_secret: config.client_secret.clone().expect("startup"),

          auth_url,
          token_url,
          user_api_url,
          scopes: if config.scopes.is_empty() {
            None
          } else {
            Some(config.scopes.clone())
          },
        }))
      }),
    }
  }
}

// Reference: https://openid.net/specs/openid-connect-core-1_0.html#StandardClaims
#[derive(Default, Debug, Deserialize, Serialize)]
pub struct OidcUser {
  pub sub: String,
  /// Requires the `email` scope. Absent for providers that don't offer it.
  pub email: Option<String>,
  pub email_verified: Option<bool>,
  /// Requires the `profile` scope.
  pub preferred_username: Option<String>,
  pub picture: Option<String>,
}

#[async_trait]
impl OAuthProvider for OidcProvider {
  fn name(&self) -> &str {
    return &self.name;
  }

  fn provider(&self) -> proto::OAuthProviderId {
    return proto::OAuthProviderId::Oidc0;
  }

  fn display_name(&self) -> &str {
    return &self.display_name;
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    return Ok(OAuthClientSettings {
      auth_url: Url::parse(&self.auth_url).map_err(|err| AuthError::Internal(err.into()))?,
      token_url: Url::parse(&self.token_url).map_err(|err| AuthError::Internal(err.into()))?,
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self, user_identifier: proto::UserIdentifier) -> Vec<String> {
    return self.scopes.as_ref().map_or_else(
      || match user_identifier {
        proto::UserIdentifier::OnlyUsername => vec!["openid".to_string(), "profile".to_string()],
        _ => vec![
          "openid".to_string(),
          "email".to_string(),
          "profile".to_string(),
        ],
      },
      |scopes| scopes.clone(),
    );
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
      .get(&self.user_api_url)
      .bearer_auth(token_response.access_token().secret())
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    let user = response
      .json::<OidcUser>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    return Ok(OAuthUser {
      provider_user_id: user.sub,
      provider_id: proto::OAuthProviderId::Oidc0,
      email: user.email,
      username: user.preferred_username,
      verified: user.email_verified.unwrap_or(true),
      avatar: user.picture,
    });
  }
}
