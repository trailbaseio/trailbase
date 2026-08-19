use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::OAuthUser;
use crate::auth::oauth::provider::UserIdentifier;
use crate::auth::oauth::providers::OAuthProviderError;
use crate::auth::oauth::simple_provider::SimpleOAuthProvider;
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

// Available fields:
// * https://yandex.com/dev/id/doc/en/user-information
// * https://authjs.dev/reference/core/providers/yandex.
#[derive(Default, Deserialize, Debug)]
pub struct YandexUser {
  id: String,
  // real_name: String,
  login: Option<String>,
  default_email: String,
  is_avatar_empty: bool,
  default_avatar_id: String,
}

impl TryFrom<YandexUser> for OAuthUser {
  type Error = AuthError;

  fn try_from(user: YandexUser) -> Result<Self, Self::Error> {
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
      email: Some(user.default_email),
      username: user.login,
      verified: true,
      avatar,
    });
  }
}

pub(crate) struct YandexOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl SimpleOAuthProvider for YandexOAuthProvider {
  const ID: OAuthProviderId = OAuthProviderId::Yandex;
  const NAME: &'static str = "yandex";
  const DISPLAY_NAME: &'static str = "Yandex";

  const AUTH_URL: &'static str = "https://oauth.yandex.com/authorize";
  const TOKEN_URL: &'static str = "https://oauth.yandex.com/token";
  const USER_API_URL: &'static str = "https://login.yandex.ru/info";

  type User = YandexUser;

  fn client_id(&self) -> String {
    return self.client_id.clone();
  }

  fn client_secret(&self) -> String {
    return self.client_secret.clone();
  }

  fn oauth_scopes(&self, _: UserIdentifier) -> Vec<String> {
    return vec![
      "login:email".to_string(),
      "login:avatar".to_string(),
      "login:info".to_string(),
    ];
  }

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
}
