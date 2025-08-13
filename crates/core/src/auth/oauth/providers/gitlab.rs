use async_trait::async_trait;
use lazy_static::lazy_static;
use oauth2::TokenResponse as _;
use serde::Deserialize;
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::provider::TokenResponse;
use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderFactory};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

pub(crate) struct GitlabOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl GitlabOAuthProvider {
  const NAME: &'static str = "gitlab";
  const DISPLAY_NAME: &'static str = "GitLab";

  const AUTH_URL: &'static str = "https://gitlab.com/oauth/authorize";
  const TOKEN_URL: &'static str = "https://gitlab.com/oauth/token";
  const USER_API_URL: &'static str = "https://gitlab.com/api/v4/user";

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing("GitLab client id".to_string()));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "GitLab client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }

  pub fn factory() -> OAuthProviderFactory {
    OAuthProviderFactory {
      id: OAuthProviderId::Gitlab,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }
}

#[async_trait]
impl OAuthProvider for GitlabOAuthProvider {
  fn name(&self) -> &'static str {
    Self::NAME
  }
  fn provider(&self) -> OAuthProviderId {
    OAuthProviderId::Gitlab
  }
  fn display_name(&self) -> &'static str {
    Self::DISPLAY_NAME
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    lazy_static! {
      static ref AUTH_URL: Url = Url::parse(GitlabOAuthProvider::AUTH_URL).expect("infallible");
      static ref TOKEN_URL: Url = Url::parse(GitlabOAuthProvider::TOKEN_URL).expect("infallible");
    }

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self) -> Vec<&'static str> {
    return vec!["read_user"];
  }

  async fn get_user(&self, token_response: &TokenResponse) -> Result<OAuthUser, AuthError> {
    if *token_response.token_type() != oauth2::basic::BasicTokenType::Bearer {
      return Err(AuthError::Internal(
        format!("Unexpected token type: {:?}", token_response.token_type()).into(),
      ));
    }

    let response = reqwest::Client::new()
      .get(Self::USER_API_URL)
      .bearer_auth(token_response.access_token().secret())
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    // https://docs.gitlab.com/ee/api/users.html#for-user
    #[derive(Default, Deserialize, Debug)]
    struct GitlabUser {
      id: i64,
      // name: String,
      // username: String,
      email: String,
      avatar_url: Option<String>,
      state: String,
    }

    let user = response
      .json::<GitlabUser>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;
    let verified = user.state == "active";
    if !verified {
      return Err(AuthError::Unauthorized);
    }

    return Ok(OAuthUser {
      provider_user_id: user.id.to_string(),
      provider_id: OAuthProviderId::Gitlab,
      email: user.email,
      verified,
      avatar: user.avatar_url,
    });
  }
}
