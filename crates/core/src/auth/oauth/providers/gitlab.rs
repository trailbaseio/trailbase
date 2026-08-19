use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::OAuthUser;
use crate::auth::oauth::provider::UserIdentifier;
use crate::auth::oauth::providers::OAuthProviderError;
use crate::auth::oauth::simple_provider::SimpleOAuthProvider;
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

// https://docs.gitlab.com/ee/api/users.html#for-user
#[derive(Default, Deserialize, Debug)]
pub struct GitlabUser {
  id: i64,
  // name: String,
  username: Option<String>,
  email: String,
  avatar_url: Option<String>,
  state: String,
}

impl TryFrom<GitlabUser> for OAuthUser {
  type Error = AuthError;

  fn try_from(user: GitlabUser) -> Result<Self, Self::Error> {
    let verified = user.state == "active";
    return Ok(OAuthUser {
      provider_user_id: user.id.to_string(),
      provider_id: OAuthProviderId::Gitlab,
      email: Some(user.email),
      username: user.username,
      verified,
      avatar: user.avatar_url,
    });
  }
}

pub(crate) struct GitlabOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl SimpleOAuthProvider for GitlabOAuthProvider {
  const ID: OAuthProviderId = OAuthProviderId::Gitlab;
  const NAME: &'static str = "gitlab";
  const DISPLAY_NAME: &'static str = "GitLab";

  const AUTH_URL: &'static str = "https://gitlab.com/oauth/authorize";
  const TOKEN_URL: &'static str = "https://gitlab.com/oauth/token";
  const USER_API_URL: &'static str = "https://gitlab.com/api/v4/user";

  type User = GitlabUser;

  fn client_id(&self) -> String {
    return self.client_id.clone();
  }

  fn client_secret(&self) -> String {
    return self.client_secret.clone();
  }

  fn oauth_scopes(&self, _: UserIdentifier) -> Vec<String> {
    // TODO: Pick scopes based on user-id policy.
    return vec!["read_user".to_string()];
  }

  fn factory(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
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
}
