use async_trait::async_trait;
use lazy_static::lazy_static;
use serde::Deserialize;
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::provider::TokenResponse;
use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderFactory};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

pub(crate) struct AppleOAuthProvider {
  client_id: String,
  client_secret: String,
}

#[allow(unused)]
#[derive(Debug, Deserialize)]
struct ApplePublicKey {
  kty: String,
  kid: String,
  #[serde(rename = "use")]
  key_use: String,
  alg: String,
  n: String,
  e: String,
}

#[derive(Debug, Deserialize)]
struct ApplePublicKeys {
  keys: Vec<ApplePublicKey>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AppleIdToken {
  pub sub: String,
  pub email: Option<String>,
  pub email_verified: Option<String>,
  // ...Other fields, e.g.:
  // pub aud: String,
  // pub iss: String,
  // pub exp: i64,
  // pub iat: i64,
}

/// Apple OAuth2 provider, also known as "Sign-in with Apple".
impl AppleOAuthProvider {
  const NAME: &'static str = "apple";
  const DISPLAY_NAME: &'static str = "Apple";

  // Unlike most other OAuth provider, Apple doesn't have a user api, but rather puts claims in the
  // JWT id_token.
  const AUTH_URL: &str = "https://appleid.apple.com/auth/authorize";
  const TOKEN_URL: &str = "https://appleid.apple.com/auth/token";

  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    let Some(client_id) = config.client_id.clone() else {
      return Err(OAuthProviderError::Missing("Apple client id".to_string()));
    };
    let Some(client_secret) = config.client_secret.clone() else {
      return Err(OAuthProviderError::Missing(
        "Apple client secret".to_string(),
      ));
    };

    return Ok(Self {
      client_id,
      client_secret,
    });
  }

  pub fn factory() -> OAuthProviderFactory {
    OAuthProviderFactory {
      id: OAuthProviderId::Apple,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }

  async fn verify_apple_id_token(&self, id_token: &str) -> Result<AppleIdToken, AuthError> {
    let header = jsonwebtoken::decode_header(id_token)
      .map_err(|err| AuthError::FailedDependency(err.into()))?;
    let Some(kid) = header.kid else {
      return Err(AuthError::FailedDependency(
        "Missing kid in token header".into(),
      ));
    };

    // TODO: Should maybe cache the JWK responses.
    let public_keys = fetch_apple_public_keys().await?;

    // Find the key.
    let Some(public_key) = public_keys.keys.iter().find(|key| key.kid == kid) else {
      return Err(AuthError::Unauthorized);
    };

    let decoding_key = jsonwebtoken::DecodingKey::from_rsa_components(&public_key.n, &public_key.e)
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_audience(&[&self.client_id]);
    validation.set_issuer(&["https://appleid.apple.com"]);

    let token_data = jsonwebtoken::decode::<AppleIdToken>(id_token, &decoding_key, &validation)
      .map_err(|err| AuthError::FailedDependency(err.into()))?;

    return Ok(token_data.claims);
  }
}

#[async_trait]
impl OAuthProvider for AppleOAuthProvider {
  fn name(&self) -> &'static str {
    Self::NAME
  }
  fn provider(&self) -> OAuthProviderId {
    OAuthProviderId::Apple
  }
  fn display_name(&self) -> &'static str {
    Self::DISPLAY_NAME
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    lazy_static! {
      static ref AUTH_URL: Url = Url::parse(AppleOAuthProvider::AUTH_URL).expect("infallible");
      static ref TOKEN_URL: Url = Url::parse(AppleOAuthProvider::TOKEN_URL).expect("infallible");
    }

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self) -> Vec<&'static str> {
    return vec!["name", "email"];
  }

  async fn get_user(&self, token_response: &TokenResponse) -> Result<OAuthUser, AuthError> {
    let Some(ref id_token) = token_response.extra_fields().id_token else {
      return Err(AuthError::BadRequest("missing id token"));
    };

    let apple_id_token = self.verify_apple_id_token(id_token).await?;

    let Some(email) = apple_id_token.email else {
      return Err(AuthError::BadRequest("missing email"));
    };

    return Ok(OAuthUser {
      provider_user_id: apple_id_token.sub,
      provider_id: OAuthProviderId::Apple,
      email,
      verified: apple_id_token.email_verified.is_some_and(|v| v == "true"),
      avatar: None,
    });
  }
}

async fn fetch_apple_public_keys() -> Result<ApplePublicKeys, AuthError> {
  const JWK_URL: &str = "https://appleid.apple.com/auth/keys";

  let http_client = reqwest::ClientBuilder::new()
    // Following redirects might set us up for server-side request forgery (SSRF).
    .redirect(reqwest::redirect::Policy::none())
    .build()
    .map_err(|err| AuthError::Internal(err.into()))?;

  let response = http_client
    .get(JWK_URL)
    .send()
    .await
    .map_err(|err| AuthError::FailedDependency(err.into()))?;

  return response
    .json()
    .await
    .map_err(|err| AuthError::FailedDependency(err.into()));
}
