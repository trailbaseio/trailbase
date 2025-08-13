use async_trait::async_trait;
use oauth2::{
  AuthUrl, Client, ClientId, ClientSecret, EndpointNotSet, EndpointSet, RedirectUrl,
  StandardRevocableToken, TokenUrl,
};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::config::proto::OAuthProviderId;
use crate::constants::AUTH_API_PATH;

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

#[derive(Serialize, Deserialize, Debug)]
pub struct OAuthUser {
  pub provider_user_id: String,
  pub provider_id: OAuthProviderId,

  pub email: String,
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

#[async_trait]
pub trait OAuthProvider {
  #[allow(unused)]
  fn provider(&self) -> OAuthProviderId;

  fn name(&self) -> &str;

  fn display_name(&self) -> &str;

  fn settings(&self) -> Result<OAuthClientSettings, AuthError>;

  fn oauth_client(&self, state: &AppState) -> Result<OAuthClient, AuthError> {
    let Some(ref site_url) = *state.site_url() else {
      return Err(AuthError::Internal(
        "Missing site_url for redirect back from external provider to your TB instance".into(),
      ));
    };

    let redirect_url: Url = site_url
      .join(&format!(
        "/{AUTH_API_PATH}/oauth/{name}/callback",
        name = self.name()
      ))
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    let settings = self.settings()?;
    if settings.client_id.is_empty() {
      return Err(AuthError::Internal(
        format!("Missing client id for {}", self.name()).into(),
      ));
    }
    if settings.client_secret.is_empty() {
      return Err(AuthError::Internal(
        format!("Missing client secret for {}", self.name()).into(),
      ));
    }

    let client = Client::new(ClientId::new(settings.client_id))
      .set_client_secret(ClientSecret::new(settings.client_secret))
      .set_auth_uri(AuthUrl::from_url(settings.auth_url))
      .set_token_uri(TokenUrl::from_url(settings.token_url))
      .set_redirect_uri(RedirectUrl::from_url(redirect_url));

    return Ok(client);
  }

  fn oauth_scopes(&self) -> Vec<&'static str>;

  //async fn get_user(&self, access_token: &oauth2::AccessToken) -> Result<OAuthUser, AuthError>;
  async fn get_user(&self, token_response: &TokenResponse) -> Result<OAuthUser, AuthError>;
}
