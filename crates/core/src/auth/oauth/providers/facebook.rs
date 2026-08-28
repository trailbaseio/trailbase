use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::OAuthUser;
use crate::auth::oauth::providers::OAuthProviderError;
use crate::auth::oauth::simple_provider::SimpleOAuthProvider;
use crate::config::proto;

#[derive(Default, Deserialize, Debug)]
pub struct FacebookUserPictureData {
  url: String,
}

#[derive(Default, Deserialize, Debug)]
pub struct FacebookUserPicture {
  data: FacebookUserPictureData,
}

// https://developers.facebook.com/docs/graph-api/reference/user/#default-public-profile-fields
#[derive(Default, Deserialize, Debug)]
pub struct FacebookUser {
  id: String,
  email: String,
  // name: Option<String>,
  picture: Option<FacebookUserPicture>,
}

impl TryFrom<FacebookUser> for OAuthUser {
  type Error = AuthError;

  fn try_from(user: FacebookUser) -> Result<Self, Self::Error> {
    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: proto::OAuthProviderId::Facebook,
      email: Some(user.email),
      username: None,
      verified: true,
      avatar: user.picture.map(|p| p.data.url),
    });
  }
}

pub(crate) struct FacebookOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl SimpleOAuthProvider for FacebookOAuthProvider {
  const ID: proto::OAuthProviderId = proto::OAuthProviderId::Facebook;
  const NAME: &'static str = "facebook";
  const DISPLAY_NAME: &'static str = "Facebook";

  const AUTH_URL: &'static str = "https://www.facebook.com/v3.2/dialog/oauth";
  const TOKEN_URL: &'static str = "https://graph.facebook.com/v3.2/oauth/access_token";
  const USER_API_URL: &'static str =
    "https://graph.facebook.com/me?fields=name,email,picture.type(large)";

  type User = FacebookUser;

  fn client_id(&self) -> String {
    return self.client_id.clone();
  }

  fn client_secret(&self) -> String {
    return self.client_secret.clone();
  }

  fn oauth_scopes(&self, _: proto::UserIdentifier) -> Vec<String> {
    // TODO: Pick scopes based on user-id policy.
    return vec!["email".to_string()];
  }

  fn new(config: &proto::OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing(
        "Facebook client id".to_string(),
      ));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Facebook client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }
}
