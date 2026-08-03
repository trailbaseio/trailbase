use async_trait::async_trait;
use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::providers::client::UserApi;
use crate::auth::oauth::providers::social::{ExternalUser, SocialSpec};
use crate::config::proto::OAuthProviderId;

pub(crate) struct Google;

#[derive(Default, Deserialize, Debug)]
pub(crate) struct GoogleUser {
  id: String,
  // name: Option<String>,
  email: String,
  verified_email: bool,
  picture: Option<String>,
}

#[async_trait]
impl SocialSpec for Google {
  const ID: OAuthProviderId = OAuthProviderId::Google;
  const NAME: &'static str = "google";
  const DISPLAY_NAME: &'static str = "Google";

  const AUTH_URL: &'static str = "https://accounts.google.com/o/oauth2/auth";
  const TOKEN_URL: &'static str = "https://accounts.google.com/o/oauth2/token";
  const USER_API_URL: &'static str = "https://www.googleapis.com/oauth2/v1/userinfo";

  const SCOPES: &'static [&'static str] = &[
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/userinfo.email",
  ];

  type User = GoogleUser;

  async fn map_user(_api: &UserApi<'_>, user: GoogleUser) -> Result<ExternalUser, AuthError> {
    return Ok(ExternalUser {
      provider_user_id: user.id,
      email: Some(user.email),
      verified: user.verified_email,
      avatar: user.picture,
      ..Default::default()
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::auth::oauth::providers::social::resolve_user;

  #[tokio::test]
  async fn test_google_user_mapping() {
    let user = resolve_user::<Google>(serde_json::json!({
      "id": "1234",
      "email": "user@gmail.com",
      "verified_email": true,
      "picture": "https://lh3.googleusercontent.com/a/avatar",
    }))
    .await
    .unwrap();

    assert_eq!(user.provider_user_id, "1234");
    assert_eq!(user.email.as_deref(), Some("user@gmail.com"));
    assert!(user.verified);
    assert_eq!(
      user.avatar.as_deref(),
      Some("https://lh3.googleusercontent.com/a/avatar")
    );
  }

  #[tokio::test]
  async fn test_google_rejects_unverified_email() {
    let result = resolve_user::<Google>(serde_json::json!({
      "id": "1234",
      "email": "user@gmail.com",
      "verified_email": false,
    }))
    .await;

    assert!(matches!(result, Err(AuthError::Unauthorized)), "{result:?}");
  }
}
