use async_trait::async_trait;
use lazy_static::lazy_static;
use oauth2::{AuthorizationCode, PkceCodeVerifier, TokenResponse as _};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::oauth::ReqwestClient;
use crate::auth::oauth::provider::TokenResponse;
use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderFactory};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

pub(crate) struct TwitchOAuthProvider {
  client_id: String,
  client_secret: String,
}

impl TwitchOAuthProvider {
  const NAME: &'static str = "twitch";
  const DISPLAY_NAME: &'static str = "Twitch";

  const AUTH_URL: &'static str = "https://id.twitch.tv/oauth2/authorize";
  const TOKEN_URL: &'static str = "https://id.twitch.tv/oauth2/token";
  const USER_API_URL: &'static str = "https://api.twitch.tv/helix/users";

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing("Twitch client id".to_string()));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Twitch client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }

  pub fn factory() -> OAuthProviderFactory {
    OAuthProviderFactory {
      id: OAuthProviderId::Twitch,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }
}

#[async_trait]
impl OAuthProvider for TwitchOAuthProvider {
  fn name(&self) -> &'static str {
    Self::NAME
  }
  fn provider(&self) -> OAuthProviderId {
    OAuthProviderId::Twitch
  }
  fn display_name(&self) -> &'static str {
    Self::DISPLAY_NAME
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    lazy_static! {
      static ref AUTH_URL: Url = Url::parse(TwitchOAuthProvider::AUTH_URL).expect("infallible");
      static ref TOKEN_URL: Url = Url::parse(TwitchOAuthProvider::TOKEN_URL).expect("infallible");
    }

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn auth_type(&self) -> oauth2::AuthType {
    return oauth2::AuthType::RequestBody;
  }

  fn oauth_scopes(&self) -> Vec<&'static str> {
    return vec!["user:read:email"];
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
    let token_response: TokenResponse = client
      .exchange_code(AuthorizationCode::new(auth_code))
      .set_pkce_verifier(PkceCodeVerifier::new(server_pkce_code_verifier))
      .request_async(&ReqwestClient(http_client))
      .await
      .or_else(|err| match err {
        // Twitch returns non-RFC-6749 compliant body: scopes are an array rather than space
        // delimited list.
        oauth2::RequestTokenError::Parse(_path, resp) => parse_twitch_token_response(&resp),
        err => Err(AuthError::FailedDependency(err.into())),
      })?;

    return Ok(token_response);
  }

  async fn get_user(&self, token_response: &TokenResponse) -> Result<OAuthUser, AuthError> {
    if *token_response.token_type() != oauth2::basic::BasicTokenType::Bearer {
      return Err(AuthError::Internal(
        format!("Unexpected token type: {:?}", token_response.token_type()).into(),
      ));
    }

    let response = reqwest::Client::new()
      .get(Self::USER_API_URL)
      .header("Client-Id", &self.client_id)
      .bearer_auth(token_response.access_token().secret())
      .send()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    let mut users = response
      .json::<TwitchUsersResponse>()
      .await
      .map_err(|err| AuthError::FailedDependency(err.into()))?
      .data;

    let user = match users.len() {
      1 => users.swap_remove(0),
      0 => {
        return Err(AuthError::FailedDependency(
          "Twitch user response had empty data".into(),
        ));
      }
      n => {
        return Err(AuthError::FailedDependency(
          format!("Twitch user response contains {n} users").into(),
        ));
      }
    };

    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: OAuthProviderId::Twitch,
      email: user.email,
      verified: true,
      avatar: user.profile_image_url,
    });
  }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TwitchTokenResponse {
  access_token: String,
  token_type: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  expires_in: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  refresh_token: Option<String>,
  #[serde(serialize_with = "self::serialize_space_delimited_vec")]
  scopes: Option<Vec<String>>,
}

fn parse_twitch_token_response(body: &[u8]) -> Result<TokenResponse, AuthError> {
  let token_response: TwitchTokenResponse = serde_json::from_slice(body).map_err(|_err| {
    #[cfg(debug_assertions)]
    return AuthError::FailedDependency(
      format!("Invalid twitch response: {}", String::from_utf8_lossy(body)).into(),
    );

    #[cfg(not(debug_assertions))]
    return AuthError::FailedDependency("Invalid twitch response".into());
  })?;

  return serde_json::from_value(
    serde_json::to_value(&token_response)
      .map_err(|_err| AuthError::Internal("Failed to serialize".into()))?,
  )
  .map_err(|_err| AuthError::Internal("Failed to deserialize".into()));
}

// Reference: https://dev.twitch.tv/docs/api/reference#get-users
#[derive(Default, Deserialize, Debug)]
struct TwitchUser {
  id: String,
  // According to reference above, email is implicitly verified.
  email: String,
  // login: String,
  // display_name: String,
  profile_image_url: Option<String>,
}

#[derive(Deserialize, Debug)]
struct TwitchUsersResponse {
  data: Vec<TwitchUser>,
}

pub fn serialize_space_delimited_vec<T, S>(
  vec_opt: &Option<Vec<T>>,
  serializer: S,
) -> Result<S::Ok, S::Error>
where
  T: AsRef<str>,
  S: serde::ser::Serializer,
{
  if let Some(ref vec) = *vec_opt {
    let space_delimited = vec.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join(" ");
    serializer.serialize_str(&space_delimited)
  } else {
    serializer.serialize_none()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_twitch_token_response_test() {
    let response = r#"{
      "access_token": "xxx",
      "expires_in": 13925,
      "refresh_token": "yyy",
      "scope": ["user:read:email"],
      "token_type": "bearer"
    }"#;

    parse_twitch_token_response(response.as_bytes()).unwrap();
  }
}
