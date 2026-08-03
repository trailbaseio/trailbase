use async_trait::async_trait;
use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::providers::social::{DataEnvelope, ExternalUser, SocialSpec, UserApi};
use crate::config::proto::OAuthProviderId;

pub(crate) struct Facebook;

#[derive(Default, Deserialize, Debug)]
struct FacebookPicture {
  url: String,
}

#[derive(Default, Deserialize, Debug)]
pub(crate) struct FacebookUser {
  id: String,
  email: String,
  // name: Option<String>,
  picture: Option<DataEnvelope<FacebookPicture>>,
}

#[async_trait]
impl SocialSpec for Facebook {
  const ID: OAuthProviderId = OAuthProviderId::Facebook;
  const NAME: &'static str = "facebook";
  const DISPLAY_NAME: &'static str = "Facebook";

  const AUTH_URL: &'static str = "https://www.facebook.com/v3.2/dialog/oauth";
  const TOKEN_URL: &'static str = "https://graph.facebook.com/v3.2/oauth/access_token";
  const USER_API_URL: &'static str =
    "https://graph.facebook.com/me?fields=name,email,picture.type(large)";

  const SCOPES: &'static [&'static str] = &["email"];

  type User = FacebookUser;

  async fn map_user(_api: &UserApi<'_>, user: FacebookUser) -> Result<ExternalUser, AuthError> {
    return Ok(ExternalUser {
      provider_user_id: user.id,
      email: Some(user.email),
      verified: true,
      avatar: user.picture.map(|p| p.data.url),
      ..Default::default()
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::auth::oauth::providers::social::resolve_user;

  #[tokio::test]
  async fn test_facebook_user_mapping() {
    let user = resolve_user::<Facebook>(serde_json::json!({
      "id": "1234",
      "email": "user@example.com",
      "picture": { "data": { "url": "https://scontent.xx.fbcdn.net/avatar" } },
    }))
    .await
    .unwrap();

    assert_eq!(user.email.as_deref(), Some("user@example.com"));
    // The avatar is nested two levels deep in Facebook's response.
    assert_eq!(
      user.avatar.as_deref(),
      Some("https://scontent.xx.fbcdn.net/avatar")
    );
  }

  #[tokio::test]
  async fn test_facebook_user_without_picture() {
    let user = resolve_user::<Facebook>(serde_json::json!({
      "id": "1234",
      "email": "user@example.com",
    }))
    .await
    .unwrap();

    assert_eq!(user.avatar, None);
  }
}
