use async_trait::async_trait;
use oauth2::{AuthType, EndpointNotSet, EndpointSet, StandardRevocableToken};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::AuthError;
use crate::config::proto;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtraTokenFields {
  /// The `OpenID` Connect ID token returned by some providers. Expected to be in JWT format.
  pub id_token: Option<String>,
}
impl oauth2::ExtraTokenFields for ExtraTokenFields {}

pub type TokenResponse =
  oauth2::StandardTokenResponse<ExtraTokenFields, oauth2::basic::BasicTokenType>;

pub type OAuthClient<
  HasAuthUrl = EndpointSet,
  HasDeviceAuthUrl = EndpointNotSet,
  HasIntrospectionUrl = EndpointNotSet,
  HasRevocationUrl = EndpointNotSet,
  HasTokenUrl = EndpointSet,
> = oauth2::Client<
  oauth2::basic::BasicErrorResponse,
  TokenResponse,
  oauth2::basic::BasicTokenIntrospectionResponse,
  StandardRevocableToken,
  oauth2::basic::BasicRevocationErrorResponse,
  HasAuthUrl,
  HasDeviceAuthUrl,
  HasIntrospectionUrl,
  HasRevocationUrl,
  HasTokenUrl,
>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OAuthUser {
  pub provider_user_id: String,
  pub provider_id: proto::OAuthProviderId,

  pub email: Option<String>,
  pub username: Option<String>,
  pub verified: bool,

  pub avatar: Option<String>,
}

#[derive(Debug)]
pub struct OAuthClientSettings {
  pub auth_url: Url,
  pub token_url: Url,
  pub client_id: String,
  pub client_secret: String,
}

/// Common trait for OAuth providers like Discord, etc.
#[async_trait]
pub trait OAuthProvider {
  #[cfg_attr(not(test), allow(unused))]
  fn provider(&self) -> proto::OAuthProviderId;

  fn name(&self) -> &str;

  fn display_name(&self) -> &str;

  fn auth_type(&self) -> AuthType {
    AuthType::BasicAuth
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError>;

  fn oauth_scopes(&self, user_identifier: proto::UserIdentifier) -> Vec<String>;

  async fn get_user(
    &self,
    http_client: &reqwest::Client,
    token_response: &TokenResponse,
  ) -> Result<OAuthUser, AuthError>;

  fn parse_token_response(
    &self,
    #[allow(unused)] path: &serde_path_to_error::Error<serde_json::error::Error>,
    #[allow(unused)] body: &[u8],
  ) -> Result<TokenResponse, AuthError> {
    // By default OAuthProviders don't custom parse response. They expect it to be RFC-6749
    // compliant.
    #[cfg(debug_assertions)]
    return Err(AuthError::FailedDependency(
      format!("{path}: {}", String::from_utf8_lossy(body)).into(),
    ));

    #[cfg(not(debug_assertions))]
    return Err(AuthError::FailedDependency("invalid token reply".into()));
  }
}
