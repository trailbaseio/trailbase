use async_trait::async_trait;
use oauth2::TokenResponse as _;
use serde::Deserialize;
use std::sync::LazyLock;
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::provider::TokenResponse;
use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderRegistryEntry};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto;

pub(crate) struct GithubOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl GithubOAuthProvider {
  const NAME: &'static str = "github";
  const DISPLAY_NAME: &'static str = "Github";

  const AUTH_URL: &'static str = "https://github.com/login/oauth/authorize";
  const TOKEN_URL: &'static str = "https://github.com/login/oauth/access_token";
  // const DEVICE_AUTH_URL: &'static str = "https://github.com/login/device/code";
  const USER_API_URL: &'static str = "https://api.github.com/user";

  fn new(config: &proto::OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing("Github client id".to_string()));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Github client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }

  pub fn registry_entry() -> OAuthProviderRegistryEntry {
    OAuthProviderRegistryEntry {
      id: proto::OAuthProviderId::Github,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &proto::OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }
}

#[async_trait]
impl OAuthProvider for GithubOAuthProvider {
  fn name(&self) -> &'static str {
    return Self::NAME;
  }

  fn provider(&self) -> proto::OAuthProviderId {
    return proto::OAuthProviderId::Github;
  }

  fn display_name(&self) -> &'static str {
    return Self::DISPLAY_NAME;
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    static AUTH_URL: LazyLock<Url> =
      LazyLock::new(|| Url::parse(GithubOAuthProvider::AUTH_URL).expect("infallible"));
    static TOKEN_URL: LazyLock<Url> =
      LazyLock::new(|| Url::parse(GithubOAuthProvider::TOKEN_URL).expect("infallible"));

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self, _: proto::UserIdentifier) -> Vec<String> {
    // TODO: Pick scopes based on user-id policy.
    return vec!["read:user".to_string(), "user:email".to_string()];
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

    //  Checkout available fields on: https://docs.github.com/en/rest/users/users?apiVersion=2026-03-10
    #[derive(Default, Deserialize, Debug)]
    struct GithubUser {
      id: i64,
      login: Option<String>,
      // name: String,
      email: Option<String>,
      // verified: bool,
      avatar_url: Option<String>,
    }

    let response = http_client
      .get(Self::USER_API_URL)
      .bearer_auth(token_response.access_token().secret())
      .header(axum::http::header::USER_AGENT, "TrailBase")
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    let user = response
      .json::<GithubUser>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    // Users can set the "Keep my email private" option, in which case the user api will return an
    // empty email and we'll have to call the dedicated `/emails` endpoint.
    let email = if let Some(email) = user.email
      && !email.is_empty()
    {
      email
    } else {
      #[allow(non_snake_case)]
      #[derive(Default, Deserialize, Debug)]
      struct GithubEmail {
        email: String,
        primary: bool,
        verified: bool,
        // NOTE: null | "private" | "public"
        // visibility: Option<String>,
      }

      let email_response = http_client
        .get(format!("{}/emails", Self::USER_API_URL))
        .bearer_auth(token_response.access_token().secret())
        .header(axum::http::header::USER_AGENT, "TrailBase")
        .send()
        .await
        .map_err(|err| AuthError::FailedDependency(err.into()))?;

      let emails: Vec<GithubEmail> = email_response
        .json()
        .await
        .map_err(|err| AuthError::FailedDependency(err.into()))?;

      let Some(primary) = emails
        .into_iter()
        .find(|cand| cand.verified && cand.primary)
      else {
        return Err(AuthError::FailedDependency("missing email".into()));
      };

      primary.email
    };

    return Ok(OAuthUser {
      provider_user_id: user.id.to_string(),
      provider_id: proto::OAuthProviderId::Github,
      email: Some(email),
      username: user.login,
      verified: true,
      avatar: user.avatar_url,
    });
  }
}
