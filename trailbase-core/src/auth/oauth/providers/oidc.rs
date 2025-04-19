use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::providers::OAuthProviderFactory;
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

// TODO: Add name/display name and this would already be a generic CustomOAuthProvider.
pub struct OidcProvider {
  name: String,
  display_name: String,
  client_id: String,
  client_secret: String,

  auth_url: String,
  token_url: String,
  user_api_url: String,
}

impl OidcProvider {
  pub fn factory(index: u64) -> OAuthProviderFactory {
    let (id, factory_name, factory_display_name) = match index {
      0 => (OAuthProviderId::Oidc0, "oidc0", "OpenID Connect"),
      _ => panic!("Multiple OIDC provider not implemented"),
    };

    OAuthProviderFactory {
      id,
      factory_name,
      factory_display_name,
      factory: Box::new(|name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(OidcProvider {
          name: name.to_string(),
          display_name: config
            .display_name
            .as_deref()
            .unwrap_or(factory_display_name)
            .to_string(),
          client_id: config.client_id.clone().expect("startup"),
          client_secret: config.client_secret.clone().expect("startup"),

          // NOTE: the following unwraps/expects are checked for by config validation.
          auth_url: config
            .auth_url
            .as_deref()
            .expect("Auth url missing")
            .to_string(),
          token_url: config
            .token_url
            .as_deref()
            .expect("Token url missing")
            .to_string(),
          user_api_url: config
            .user_api_url
            .as_deref()
            .expect("User api url missing")
            .to_string(),
        }))
      }),
    }
  }
}

// Reference: https://openid.net/specs/openid-connect-core-1_0.html#StandardClaims
#[derive(Default, Debug, Deserialize, Serialize)]
pub struct OidcUser {
  pub sub: String,
  pub email: String,
  pub email_verified: Option<bool>,

  // pub name: Option<String>,
  // pub preferred_username : Option<String>,
  pub picture: Option<String>,
}

#[async_trait]
impl OAuthProvider for OidcProvider {
  fn name(&self) -> &str {
    return &self.name;
  }
  fn provider(&self) -> OAuthProviderId {
    OAuthProviderId::Oidc0
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

  fn oauth_scopes(&self) -> Vec<&'static str> {
    return vec!["openid", "email", "profile"];
  }

  async fn get_user(&self, access_token: String) -> Result<OAuthUser, AuthError> {
    let response = reqwest::Client::new()
      .get(&self.user_api_url)
      .bearer_auth(access_token)
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    let user = response
      .json::<OidcUser>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    return Ok(OAuthUser {
      provider_user_id: user.sub,
      provider_id: OAuthProviderId::Oidc0,
      email: user.email,
      verified: user.email_verified.unwrap_or(true),
      avatar: user.picture,
    });
  }
}
