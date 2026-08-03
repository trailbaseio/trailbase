use async_trait::async_trait;
use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::providers::social::{ExternalUser, SocialSpec, UserApi};
use crate::config::proto::OAuthProviderId;

pub(crate) struct Microsoft;

#[derive(Default, Deserialize, Debug)]
pub(crate) struct MicrosoftUser {
  id: String,
  mail: String,
  // displayName: String,
}

#[async_trait]
impl SocialSpec for Microsoft {
  const ID: OAuthProviderId = OAuthProviderId::Microsoft;
  const NAME: &'static str = "microsoft";
  const DISPLAY_NAME: &'static str = "Microsoft";

  const AUTH_URL: &'static str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
  const TOKEN_URL: &'static str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
  const USER_API_URL: &'static str = "https://graph.microsoft.com/v1.0/me";

  const SCOPES: &'static [&'static str] = &["User.Read"];

  type User = MicrosoftUser;

  async fn map_user(_api: &UserApi<'_>, user: MicrosoftUser) -> Result<ExternalUser, AuthError> {
    return Ok(ExternalUser {
      provider_user_id: user.id,
      email: Some(user.mail),
      // username: Some(user.displayName),
      verified: true,
      ..Default::default()
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::auth::oauth::providers::social::resolve_user;

  #[tokio::test]
  async fn test_microsoft_user_mapping() {
    let user = resolve_user::<Microsoft>(serde_json::json!({
      "id": "1234",
      "mail": "user@contoso.com",
      "displayName": "User",
    }))
    .await
    .unwrap();

    assert_eq!(user.provider_user_id, "1234");
    assert_eq!(user.email.as_deref(), Some("user@contoso.com"));
    assert!(user.verified);
  }
}
