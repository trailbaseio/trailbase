use async_trait::async_trait;
use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::providers::social::{ExternalUser, SocialSpec, UserApi};
use crate::config::proto::OAuthProviderId;

pub(crate) struct Gitlab;

// https://docs.gitlab.com/ee/api/users.html#for-user
#[derive(Default, Deserialize, Debug)]
pub(crate) struct GitlabUser {
  id: i64,
  // name: String,
  username: Option<String>,
  email: String,
  avatar_url: Option<String>,
  state: String,
}

#[async_trait]
impl SocialSpec for Gitlab {
  const ID: OAuthProviderId = OAuthProviderId::Gitlab;
  const NAME: &'static str = "gitlab";
  const DISPLAY_NAME: &'static str = "GitLab";

  const AUTH_URL: &'static str = "https://gitlab.com/oauth/authorize";
  const TOKEN_URL: &'static str = "https://gitlab.com/oauth/token";
  const USER_API_URL: &'static str = "https://gitlab.com/api/v4/user";

  const SCOPES: &'static [&'static str] = &["read_user"];

  type User = GitlabUser;

  async fn map_user(_api: &UserApi<'_>, user: GitlabUser) -> Result<ExternalUser, AuthError> {
    return Ok(ExternalUser {
      provider_user_id: user.id.to_string(),
      email: Some(user.email),
      username: user.username,
      // GitLab has no email-confirmation flag, but blocked and deactivated accounts must not
      // be able to log in.
      verified: user.state == "active",
      avatar: user.avatar_url,
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::auth::oauth::providers::social::resolve_user;

  #[tokio::test]
  async fn test_gitlab_user_mapping() {
    let user = resolve_user::<Gitlab>(serde_json::json!({
      "id": 42,
      "username": "john_smith",
      "email": "john@example.com",
      "avatar_url": "https://gitlab.com/uploads/user/avatar/42/index.jpg",
      "state": "active",
    }))
    .await
    .unwrap();

    // GitLab ids are numeric, ours are strings.
    assert_eq!(user.provider_user_id, "42");
    assert_eq!(user.username.as_deref(), Some("john_smith"));
    assert!(user.verified);
  }

  #[tokio::test]
  async fn test_gitlab_rejects_inactive_user() {
    let result = resolve_user::<Gitlab>(serde_json::json!({
      "id": 42,
      "email": "john@example.com",
      "state": "blocked",
    }))
    .await;

    assert!(matches!(result, Err(AuthError::Unauthorized)), "{result:?}");
  }
}
