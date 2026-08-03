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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OAuthUser {
  pub provider_user_id: String,
  pub provider_id: OAuthProviderId,

  /// Absent when the provider wasn't asked for, or doesn't expose, an email address. Requires a
  /// username-based `UserIdentifier`, see `create_user_for_external_provider`.
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
  /// Config key and URL path segment, i.e. what users authenticate against.
  fn name(&self) -> &str;

  /// Human-readable name, shown in the admin UI and returned by the providers API.
  fn display_name(&self) -> &str;

  fn auth_type(&self) -> AuthType {
    AuthType::BasicAuth
  }

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
      .set_redirect_uri(RedirectUrl::from_url(redirect_url))
      .set_auth_type(self.auth_type());

    return Ok(client);
  }

  /// Scopes to request from the provider.
  ///
  /// NOTE: Tied to `&self`'s lifetime rather than `'static`, so providers can return scopes
  /// that were read from the config.
  fn oauth_scopes(&self) -> Vec<&str>;

  /// Salvages a token response that failed to parse because the provider doesn't comply with
  /// RFC-6749. Returning `None` propagates the original parse error.
  fn recover_token_response(&self, _body: &[u8]) -> Option<Result<TokenResponse, AuthError>> {
    return None;
  }

  async fn get_token(
    &self,
    state: &AppState,
    auth_code: String,
    server_pkce_code_verifier: String,
  ) -> Result<TokenResponse, AuthError> {
    let http_client = reqwest::ClientBuilder::new()
      // Following redirects might set us up for server-side request forgery (SSRF).
      .redirect(reqwest::redirect::Policy::none())
      .build()
      .map_err(|err| AuthError::Internal(err.into()))?;

    let client = self.oauth_client(state)?;
    return client
      .exchange_code(AuthorizationCode::new(auth_code))
      .set_pkce_verifier(PkceCodeVerifier::new(server_pkce_code_verifier))
      .request_async(&ReqwestClient(http_client))
      .await
      .or_else(|err| {
        if let oauth2::RequestTokenError::Parse(ref _path, ref body) = err
          && let Some(recovered) = self.recover_token_response(body)
        {
          return recovered;
        }

        #[cfg(debug_assertions)]
        return Err(match err {
          oauth2::RequestTokenError::Parse(_path, resp) => {
            AuthError::Internal(String::from_utf8_lossy(&resp).into())
          }
          err => AuthError::FailedDependency(format!("{err:?}").into()),
        });

        #[cfg(not(debug_assertions))]
        return Err(AuthError::FailedDependency(err.into()));
      });
  }

  async fn get_user(&self, token_response: &TokenResponse) -> Result<OAuthUser, AuthError>;
}
