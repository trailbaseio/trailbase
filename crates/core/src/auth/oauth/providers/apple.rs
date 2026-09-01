use async_trait::async_trait;
use serde::Deserialize;
use std::sync::LazyLock;
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::provider::TokenResponse;
use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderRegistryEntry};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto;

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
#[serde(untagged)]
pub enum Boolean {
  String(String),
  Bool(bool),
}

impl Boolean {
  fn value(&self) -> bool {
    return match self {
      Boolean::Bool(v) => *v,
      Boolean::String(s) if s.to_lowercase() == "true" => true,
      Boolean::String(_) => false,
    };
  }
}

#[derive(Clone, Debug, Deserialize)]
pub struct AppleIdToken {
  pub sub: String,
  pub email: Option<String>,
  pub email_verified: Option<Boolean>,
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

  fn new(config: &proto::OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
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

  pub fn registry_entry() -> OAuthProviderRegistryEntry {
    OAuthProviderRegistryEntry {
      id: proto::OAuthProviderId::Apple,
      factory_name: Self::NAME,
      factory_display_name: Self::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &proto::OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    }
  }

  async fn verify_apple_id_token(
    &self,
    http_client: &reqwest::Client,
    id_token: &str,
  ) -> Result<AppleIdToken, AuthError> {
    let header = jsonwebtoken::decode_header(id_token)
      .map_err(|err| AuthError::FailedDependency(err.into()))?;
    let Some(kid) = header.kid else {
      return Err(AuthError::FailedDependency(
        "Missing kid in token header".into(),
      ));
    };

    // TODO: Should maybe cache the JWK responses.
    let public_keys = fetch_apple_public_keys(http_client).await?;

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
    return Self::NAME;
  }

  fn provider(&self) -> proto::OAuthProviderId {
    return proto::OAuthProviderId::Apple;
  }

  fn display_name(&self) -> &'static str {
    return Self::DISPLAY_NAME;
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    static AUTH_URL: LazyLock<Url> = LazyLock::new(|| {
      // When scopes "name" and/or "email" are specified, apple expects `response_mode=form_post`
      // and to call-back using a POST method:
      //   https://developer.apple.com/documentation/signinwithapple/incorporating-sign-in-with-apple-into-other-platforms
      const AUTH_URL: &str = "https://appleid.apple.com/auth/authorize?response_mode=form_post";
      return Url::parse(AUTH_URL).expect("tested");
    });
    static TOKEN_URL: LazyLock<Url> = LazyLock::new(|| {
      const TOKEN_URL: &str = "https://appleid.apple.com/auth/token";
      return Url::parse(TOKEN_URL).expect("tested");
    });

    return Ok(OAuthClientSettings {
      auth_url: AUTH_URL.clone(),
      token_url: TOKEN_URL.clone(),
      client_id: self.client_id.clone(),
      client_secret: self.client_secret.clone(),
    });
  }

  fn oauth_scopes(&self, _: proto::UserIdentifier) -> Vec<String> {
    // TODO: Pick scopes based on user-id policy.
    return vec!["name".to_string(), "email".to_string()];
  }

  /// Unlike most other OAuth provider, Apple doesn't have a user api, but rather puts claims in
  /// the JWT id_token.
  async fn get_user(
    &self,
    http_client: &reqwest::Client,
    token_response: &TokenResponse,
  ) -> Result<OAuthUser, AuthError> {
    let Some(ref id_token) = token_response.extra_fields().id_token else {
      return Err(AuthError::BadRequest("missing id token"));
    };

    let apple_id_token = self.verify_apple_id_token(http_client, id_token).await?;

    let Some(email) = apple_id_token.email else {
      return Err(AuthError::BadRequest("missing email"));
    };

    return Ok(OAuthUser {
      provider_user_id: apple_id_token.sub,
      provider_id: proto::OAuthProviderId::Apple,
      email: Some(email),
      username: None,
      verified: apple_id_token.email_verified.is_some_and(|v| v.value()),
      avatar: None,
    });
  }
}

async fn fetch_apple_public_keys(
  http_client: &reqwest::Client,
) -> Result<ApplePublicKeys, AuthError> {
  const JWK_URL: &str = "https://appleid.apple.com/auth/keys";

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

#[cfg(test)]
mod tests {
  use serde_json::{from_value, json};

  use super::*;

  #[test]
  fn test_apple_settings() {
    let provider = AppleOAuthProvider {
      client_id: "12345".to_string(),
      client_secret: "s3cre7".to_string(),
    };

    let settings = provider.settings().unwrap();
    let query: Vec<_> = settings.auth_url.query_pairs().collect();
    assert!(!query.is_empty());
  }

  #[test]
  fn test_apple_boolean() {
    // Apple may return strings or booleans: https://developer.apple.com/forums/thread/746352
    let v0 = from_value::<AppleIdToken>(json!({
            "sub": "123",
            "email_verified": "TruE",
    }))
    .unwrap();
    assert_eq!(true, v0.email_verified.unwrap().value());

    let v1 = from_value::<AppleIdToken>(json!({
            "sub": "123",
            "email_verified": "Anything Else",
    }))
    .unwrap();
    assert_eq!(false, v1.email_verified.unwrap().value());

    let v2 = from_value::<AppleIdToken>(json!({
            "sub": "123",
            "email_verified": false,
    }))
    .unwrap();
    assert_eq!(false, v2.email_verified.unwrap().value());

    let v3 = from_value::<AppleIdToken>(json!({
            "sub": "123",
            "email_verified": true,
    }))
    .unwrap();
    assert_eq!(true, v3.email_verified.unwrap().value());
  }
}
