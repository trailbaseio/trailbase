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

pub(crate) struct YandexOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl YandexOAuthProvider {
  const NAME: &'static str = "yandex";
  const DISPLAY_NAME: &'static str = "Yandex";

  const AUTH_URL: &'static str = "https://oauth.yandex.com/authorize";
  const TOKEN_URL: &'static str = "https://oauth.yandex.com/token";
  const USER_API_URL: &'static str = "https://login.yandex.ru/info";

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing("Yandex client id".to_string()));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Yandex client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }

  pub fn factory() -> OAuthProviderFactory {
    OAuthProviderFactory {
      id: OAuthProviderId::Yandex,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }
}

#[async_trait]
impl OAuthProvider for YandexOAuthProvider {
  fn name(&self) -> &'static str {
    return Self::NAME;
  }

  fn provider(&self) -> OAuthProviderId {
    return OAuthProviderId::Yandex;
  }

  fn display_name(&self) -> &'static str {
    return Self::DISPLAY_NAME;
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    lazy_static! {
      static ref AUTH_URL: Url = Url::parse(YandexOAuthProvider::AUTH_URL).expect("infallible");
      static ref TOKEN_URL: Url = Url::parse(YandexOAuthProvider::TOKEN_URL).expect("infallible");
    }

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self) -> Vec<&'static str> {
    return vec!["login:email", "login:avatar", "login:info"];
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

    // Checkout available fields on:
    // * https://yandex.com/dev/id/doc/en/user-information
    // * https://authjs.dev/reference/core/providers/yandex.
    #[derive(Default, Deserialize, Debug)]
    struct YandexUser {
      id: String,
      // real_name: String,
      // login: String,
      default_email: String,
      is_avatar_empty: bool,
      default_avatar_id: String,
    }

    let user = response
      .json::<YandexUser>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    let avatar = if !user.is_avatar_empty {
      Some(format!(
        "https://avatars.yandex.net/get-yapic/{}/islands-200",
        user.default_avatar_id
      ))
    } else {
      None
    };

    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: OAuthProviderId::Yandex,
      email: user.default_email,
      verified: true,
      avatar,
    });
  }
}
