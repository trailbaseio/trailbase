use async_trait::async_trait;
use oauth2::{
  AuthType, AuthUrl, AuthorizationCode, Client, ClientId, ClientSecret, EndpointNotSet,
  EndpointSet, PkceCodeVerifier, RedirectUrl, StandardRevocableToken, TokenUrl,
};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::oauth::ReqwestClient;
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

pub use crate::config::proto::UserIdentifier;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OAuthUser {
  pub provider_user_id: String,
  pub provider_id: OAuthProviderId,

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
  fn provider(&self) -> OAuthProviderId;

  fn name(&self) -> &str;

  fn display_name(&self) -> &str;

  fn auth_type(&self) -> AuthType {
    AuthType::BasicAuth
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError>;

  fn oauth_scopes(&self, user_identifier: UserIdentifier) -> Vec<String>;

  async fn get_user(
    &self,
    http_client: &reqwest::Client,
    token_response: &TokenResponse,
  ) -> Result<OAuthUser, AuthError>;

  // TODO: We could probably turn this into a process_token response method.
  async fn get_token(
    &self,
    http_client: &reqwest::Client,
    oauth_client: OAuthClient,
    auth_code: String,
    server_pkce_code_verifier: String,
  ) -> Result<TokenResponse, AuthError> {
    return oauth_client
      .exchange_code(AuthorizationCode::new(auth_code))
      .set_pkce_verifier(PkceCodeVerifier::new(server_pkce_code_verifier))
      .request_async(&ReqwestClient(http_client))
      .await
      .map_err(|err| {
        #[cfg(debug_assertions)]
        return match err {
          oauth2::RequestTokenError::Parse(_path, resp) => {
            AuthError::Internal(String::from_utf8_lossy(&resp).into())
          }
          err => AuthError::FailedDependency(format!("{err:?}").into()),
        };

        #[cfg(not(debug_assertions))]
        return AuthError::FailedDependency(err.into());
      });
  }
}

pub(crate) fn build_oauth_client(
  state: &AppState,
  provider: &(dyn OAuthProvider + Send + Sync),
) -> Result<OAuthClient, AuthError> {
  let Some(ref site_url) = *state.site_url() else {
    return Err(AuthError::Internal(
      "Missing site_url for redirect back from external provider to your TB instance".into(),
    ));
  };

  let name = provider.name();
  let redirect_url: Url = site_url
    .join(&format!("/{AUTH_API_PATH}/oauth/{name}/callback",))
    .map_err(|err| AuthError::FailedDependency(err.into()))?;

  let settings = provider.settings()?;
  if settings.client_id.is_empty() {
    return Err(AuthError::Internal(
      format!("Missing client id for {name}").into(),
    ));
  }
  if settings.client_secret.is_empty() {
    return Err(AuthError::Internal(
      format!("Missing client secret for {name}").into(),
    ));
  }

  return Ok(
    Client::new(ClientId::new(settings.client_id))
      .set_client_secret(ClientSecret::new(settings.client_secret))
      .set_auth_uri(AuthUrl::from_url(settings.auth_url))
      .set_token_uri(TokenUrl::from_url(settings.token_url))
      .set_redirect_uri(RedirectUrl::from_url(redirect_url))
      .set_auth_type(provider.auth_type()),
  );
}
