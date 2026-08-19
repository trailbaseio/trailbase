use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::OAuthUser;
use crate::auth::oauth::provider::UserIdentifier;
use crate::auth::oauth::providers::OAuthProviderError;
use crate::auth::oauth::simple_provider::SimpleOAuthProvider;
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

// https://developers.google.com/resources/api-libraries/documentation/oauth2/v2/python/latest/oauth2_v2.userinfo.html
#[derive(Default, Deserialize, Debug)]
pub struct GoogleUser {
  id: String,
  // name: Option<String>,
  email: String,
  verified_email: bool,
  picture: Option<String>,
}

impl TryFrom<GoogleUser> for OAuthUser {
  type Error = AuthError;

  fn try_from(user: GoogleUser) -> Result<Self, Self::Error> {
    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: OAuthProviderId::Google,
      email: Some(user.email),
      username: None,
      verified: user.verified_email,
      avatar: user.picture,
    });
  }
}

pub(crate) struct GoogleOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl SimpleOAuthProvider for GoogleOAuthProvider {
  const ID: OAuthProviderId = OAuthProviderId::Google;
  const NAME: &'static str = "google";
  const DISPLAY_NAME: &'static str = "Google";

  const AUTH_URL: &'static str = "https://accounts.google.com/o/oauth2/auth";
  const TOKEN_URL: &'static str = "https://accounts.google.com/o/oauth2/token";
  const USER_API_URL: &'static str = "https://www.googleapis.com/oauth2/v1/userinfo";

  type User = GoogleUser;

  fn client_id(&self) -> String {
    return self.client_id.clone();
  }

  fn client_secret(&self) -> String {
    return self.client_secret.clone();
  }

  fn oauth_scopes(&self, _: UserIdentifier) -> Vec<String> {
    // TODO: Pick scopes based on user-id policy.
    return vec![
      "https://www.googleapis.com/auth/userinfo.profile".to_string(),
      "https://www.googleapis.com/auth/userinfo.email".to_string(),
    ];
  }

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing("Google client id".to_string()));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Google client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }
}
