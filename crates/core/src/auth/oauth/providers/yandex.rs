use async_trait::async_trait;
use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::providers::social::{ExternalUser, SocialSpec, UserApi};
use crate::config::proto::OAuthProviderId;

pub(crate) struct Yandex;

// Checkout available fields on:
// * https://yandex.com/dev/id/doc/en/user-information
// * https://authjs.dev/reference/core/providers/yandex.
#[derive(Default, Deserialize, Debug)]
pub(crate) struct YandexUser {
  id: String,
  // real_name: String,
  login: Option<String>,
  default_email: String,
  is_avatar_empty: bool,
  default_avatar_id: String,
}

#[async_trait]
impl SocialSpec for Yandex {
  const ID: OAuthProviderId = OAuthProviderId::Yandex;
  const NAME: &'static str = "yandex";
  const DISPLAY_NAME: &'static str = "Yandex";

  const AUTH_URL: &'static str = "https://oauth.yandex.com/authorize";
  const TOKEN_URL: &'static str = "https://oauth.yandex.com/token";
  const USER_API_URL: &'static str = "https://login.yandex.ru/info";

  const SCOPES: &'static [&'static str] = &["login:email", "login:avatar", "login:info"];

  type User = YandexUser;

  async fn map_user(_api: &UserApi<'_>, user: YandexUser) -> Result<ExternalUser, AuthError> {
    return Ok(ExternalUser {
      provider_user_id: user.id,
      email: Some(user.default_email),
      username: user.login,
      verified: true,
      // NOTE: Yandex sends a placeholder id alongside the flag, so the flag decides.
      avatar: (!user.is_avatar_empty).then(|| {
        format!(
          "https://avatars.yandex.net/get-yapic/{}/islands-200",
          user.default_avatar_id
        )
      }),
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::auth::oauth::providers::social::resolve_user;

  #[tokio::test]
  async fn test_yandex_user_mapping() {
    let user = resolve_user::<Yandex>(serde_json::json!({
      "id": "1234",
      "login": "ivan",
      "default_email": "ivan@yandex.ru",
      "is_avatar_empty": false,
      "default_avatar_id": "abcdef",
    }))
    .await
    .unwrap();

    assert_eq!(user.email.as_deref(), Some("ivan@yandex.ru"));
    assert_eq!(user.username.as_deref(), Some("ivan"));
    assert_eq!(
      user.avatar.as_deref(),
      Some("https://avatars.yandex.net/get-yapic/abcdef/islands-200")
    );
  }

  #[tokio::test]
  async fn test_yandex_user_without_avatar() {
    let user = resolve_user::<Yandex>(serde_json::json!({
      "id": "1234",
      "default_email": "ivan@yandex.ru",
      // Yandex sends a placeholder id alongside this flag, which must not become a URL.
      "is_avatar_empty": true,
      "default_avatar_id": "0/0-0",
    }))
    .await
    .unwrap();

    assert_eq!(user.avatar, None);
  }
}
