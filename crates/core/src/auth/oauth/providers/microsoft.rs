use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::OAuthUser;
use crate::auth::oauth::provider::UserIdentifier;
use crate::auth::oauth::providers::OAuthProviderError;
use crate::auth::oauth::simple_provider::SimpleOAuthProvider;
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

// https://learn.microsoft.com/en-us/graph/api/user-get?view=graph-rest-1.0&tabs=http#response-1
#[derive(Default, Deserialize, Debug)]
pub struct MicrosoftUser {
  id: String,
  mail: String,
  // displayName: String,
}

impl TryFrom<MicrosoftUser> for OAuthUser {
  type Error = AuthError;

  fn try_from(user: MicrosoftUser) -> Result<Self, Self::Error> {
    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: OAuthProviderId::Microsoft,
      email: Some(user.mail),
      // username: Some(user.displayName),
      username: None,
      verified: true,
      avatar: None,
    });
  }
}

pub(crate) struct MicrosoftOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl SimpleOAuthProvider for MicrosoftOAuthProvider {
  const ID: OAuthProviderId = OAuthProviderId::Microsoft;
  const NAME: &'static str = "microsoft";
  const DISPLAY_NAME: &'static str = "Microsoft";

  const AUTH_URL: &'static str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
  const TOKEN_URL: &'static str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
  const USER_API_URL: &'static str = "https://graph.microsoft.com/v1.0/me";

  type User = MicrosoftUser;

  fn client_id(&self) -> String {
    return self.client_id.clone();
  }

  fn client_secret(&self) -> String {
    return self.client_secret.clone();
  }

  fn oauth_scopes(&self, _: UserIdentifier) -> Vec<String> {
    return vec!["User.Read".to_string()];
  }

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing(
        "Microsoft client id".to_string(),
      ));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Microsoft client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }
}
