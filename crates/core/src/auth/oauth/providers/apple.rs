use async_trait::async_trait;
use serde::Deserialize;
use sha2::{Digest, Sha256};
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
pub(crate) struct ApplePublicKeys {
  keys: Vec<ApplePublicKey>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Boolean {
  String(String),
  Bool(bool),
}

impl Boolean {
  pub(crate) fn value(&self) -> bool {
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
  /// Anti-replay nonce. Only present when the authorization request carried
  /// one — always the case for the native Sign in with Apple flow, where the
  /// client sends the SHA-256 hash of its raw nonce with the request.
  #[serde(default)]
  pub nonce: Option<String>,
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
}

/// Extracts the `kid` header from a JWT without any network access.
pub(crate) fn extract_kid(id_token: &str) -> Result<String, AuthError> {
  let Some((header, _, _)) = split_jwt(id_token) else {
    return Err(AuthError::BadRequest("malformed identity token"));
  };

  #[derive(Deserialize)]
  struct Header {
    kid: Option<String>,
  }

  let header: Header = serde_json::from_slice(&header)
    .map_err(|_| AuthError::BadRequest("malformed identity token header"))?;
  return header
    .kid
    .filter(|kid| !kid.is_empty())
    .ok_or(AuthError::BadRequest(
      "identity token is missing the kid header",
    ));
}

/// Splits a compact JWT into its three base64url-decoded parts.
fn split_jwt(id_token: &str) -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)> {
  use base64::Engine as _;
  use base64::engine::general_purpose::URL_SAFE_NO_PAD;

  let mut parts = id_token.split('.');
  let decode = |part: &str| URL_SAFE_NO_PAD.decode(part).ok();
  let header = decode(parts.next()?)?;
  let payload = decode(parts.next()?)?;
  let signature = decode(parts.next()?)?;
  if parts.next().is_some() {
    return None;
  }
  return Some((header, payload, signature));
}

/// Verifies signature and claims (issuer, audience, expiry) of an Apple
/// identity token against the given public keys, selecting the key by `kid`.
pub(crate) fn decode_id_token_with_keys(
  public_keys: &ApplePublicKeys,
  id_token: &str,
  audience: &str,
) -> Result<AppleIdToken, AuthError> {
  let header =
    jsonwebtoken::decode_header(id_token).map_err(|err| AuthError::FailedDependency(err.into()))?;
  let Some(kid) = header.kid else {
    return Err(AuthError::FailedDependency(
      "Missing kid in token header".into(),
    ));
  };

  // Find the key.
  let Some(public_key) = public_keys.keys.iter().find(|key| key.kid == kid) else {
    return Err(AuthError::Unauthorized);
  };

  let decoding_key = jsonwebtoken::DecodingKey::from_rsa_components(&public_key.n, &public_key.e)
    .map_err(|err| AuthError::FailedDependency(err.into()))?;

  let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
  validation.set_audience(&[audience]);
  validation.set_issuer(&["https://appleid.apple.com"]);

  let token_data = jsonwebtoken::decode::<AppleIdToken>(id_token, &decoding_key, &validation)
    .map_err(|err| AuthError::FailedDependency(err.into()))?;

  return Ok(token_data.claims);
}

/// Checks the anti-replay nonce: the claim must be present and equal the
/// lowercase-hex SHA-256 of the client's raw nonce (the hash the client sent
/// with the authorization request).
pub(crate) fn verify_nonce_claim(
  claims_nonce: Option<&str>,
  client_nonce: &str,
) -> Result<(), AuthError> {
  let Some(claims_nonce) = claims_nonce else {
    return Err(AuthError::BadRequest("identity token carries no nonce"));
  };

  if claims_nonce != sha256_hex(client_nonce) {
    return Err(AuthError::BadRequest("nonce mismatch"));
  }
  return Ok(());
}

fn sha256_hex(value: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(value.as_bytes());
  return hasher
    .finalize()
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect();
}

#[async_trait]
impl OAuthProvider for AppleOAuthProvider {
  fn name(&self) -> &'static str {
    return Self::NAME;
  }

  fn auth_type(&self) -> oauth2::AuthType {
    // Apple only accepts client credentials in the POST body, not via HTTP Basic auth:
    // https://developer.apple.com/documentation/signinwithapple/request_and_validate_tokens
    return oauth2::AuthType::RequestBody;
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

    let public_keys = fetch_apple_public_keys(http_client).await?;
    let apple_id_token = decode_id_token_with_keys(&public_keys, id_token, &self.client_id)?;

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

// TODO: Should maybe cache the JWK responses.
#[cfg(not(test))]
pub(crate) async fn fetch_apple_public_keys(
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
pub(crate) async fn fetch_apple_public_keys(
  _http_client: &reqwest::Client,
) -> Result<ApplePublicKeys, AuthError> {
  return Ok(test_support::fixture_keys());
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

  #[test]
  fn test_apple_auth_type_is_request_body() {
    // Apple only accepts client credentials in the POST body, not HTTP Basic auth.
    let provider = AppleOAuthProvider {
      client_id: "12345".to_string(),
      client_secret: "s3cre7".to_string(),
    };

    assert!(matches!(
      provider.auth_type(),
      oauth2::AuthType::RequestBody
    ));
  }
}

/// Fixture shared by the native-verification tests here and the endpoint
/// tests in `apple_native.rs`.
#[cfg(test)]
pub(crate) mod test_support {
  use super::*;

  /// RSA keypair generated offline (openssl genrsa 2048) standing in for
  /// Apple's signing key. The JWK components below (`n`, `e`) correspond to
  /// this private key, so test tokens signed here verify against a fixture
  /// `ApplePublicKeys` exactly like production tokens against Apple's JWKS.
  pub(crate) const TEST_SIGNING_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDpJVIIFuAkQvTf\nUxobvsq9KfIZKUBB+vGI6XohqF5Ivf4/vWit+zdrm2EpwFJNVuomq+puUBrmRevM\npbHjiO6l3h691LvC7NW4gvMeEh9++00lqc6rjpDNIMDPtoHkuU6MZRiaiJGHH0pM\npX1/xNKRYV07+D8M4wwWGXCLnY8u2WiMdRhmNgb60SGilmWDWgGMLnY3vZWAJLuI\n/k9m1bPYPZxjfvf8HNFyrp62a1E6wFajYwFJkzWOvgORjMxkaPW+wwm0t+mhLUqr\nwQuxJ0sVANsoleooeXS9bF8zlIFMt0Hg0AZJ5qgpp9ScoeDINDTBUt2T2a2iggBW\nHMUfEU+XAgMBAAECggEARNpfNAFhd4QIoi1+H+SEJkJNe63JitLL4xWkmm0JTy1A\n+Vz8Hal7r/1GwBhKlgmNhBcwWByzHP9YSGtEskA9zmFfLcu2GbZs1Z1ipCZRA+S+\nX1mbLeIgFFxQZOduy/gH4QF4NycO51tPy3vyKLodP48EBFJneGxTJPGlYa4J25kL\n7wRjL4cm+HMHfYy29h0WcS5ZymQa+ZANfQl+jfMGHbM+rGMDDkcjl1XFKOEXyhU1\nhLF9enYdMTcTscVsv23chDP/YJUdWdEKV3QzY6jAYRkj9fbN4K6WcVZlhi+sn8r+\nD2UaF1HKDaGu/NFoWXHMx0y60kkhZnq2dYF9xPiGIQKBgQD20LNYRMtVy2NRfDrJ\n+xAKn6900Fj+LTk6HKcEKDd4JBY+n3Ii2MP5HvPn0w1Mf2EW8Rw4ibAKJu15mcbe\n0BwRIuu8wnaLvQaoiIKO5RoEhzvRYLWOxh6Nj/Vmtu85WVmw6Y6a+1h4C6PpQkv8\n3dWsBWszT0rf/19ip9M2yRXq4QKBgQDx0mWkbz1HEISH9NVoRGa2ZJDm38dniGwh\nPhz7X+3TQSrmR2W7N1SPrb1/rVax9lgeQOOIrCiCcNahkRCRMq2EJpdLZ4FaTsEp\nPnAjAGOKedRyXv8vMX0gQK3N+XwuVKDBFjeylL7bpr0WdZWVC6O6SYnYWaEAQwHa\njgH7Sv5BdwKBgQDZvcrKz343RURsidVvhW9kf/YRbxFjw8/dxZNOppAxDF0XiCDw\nPx289KKm3Vm5KBMmYzXLZyUH/8m3YoPA5AYu1Aj2sPRWWT+7hRrxJ4rpfci28cOa\nnowrxVnw8OhhRsNKwPGPJriox1Qmn9db0PUFWo51aLmcnbWv2nEKvyH34QKBgQCP\nhT+uCBdmRfdieXzvFSmgtq8JV2cRm3YRhLvOtXCBIPxFD7rhEkWtwH/ndwktNfe2\nfOyOAR9Jy46W9XHPuzQgaocAyb2Ly5H42IXVQDXTydq8xoTNjaGlsr10sc1x8eg2\nsOj9pCpiUuOGoOLWQsI5ncuiDA/yB9Lh08Z5Tlj4oQKBgFfWY4ymAI7LIB+QqYT3\nFuGhoj36dWC0zfE7MpNf9i0bB2IXtkaPvmQOGYY3xzf1AMV7iTwH8voPCPECoUFl\nlyJ4Vx66rC3or964SZ23+Bdr3KKxBver4aXvjJWLOO9Ow77bQT8ZOIPO8DCrByi0\nb0NLSP50ZEVOq0q09nlSfQQc\n-----END PRIVATE KEY-----";
  pub(crate) const TEST_KEY_ID: &str = "test-apple-key-1";
  pub(crate) const TEST_MODULUS: &str = "6SVSCBbgJEL031MaG77KvSnyGSlAQfrxiOl6IaheSL3-P71orfs3a5thKcBSTVbqJqvqblAa5kXrzKWx44jupd4evdS7wuzVuILzHhIffvtNJanOq46QzSDAz7aB5LlOjGUYmoiRhx9KTKV9f8TSkWFdO_g_DOMMFhlwi52PLtlojHUYZjYG-tEhopZlg1oBjC52N72VgCS7iP5PZtWz2D2cY373_BzRcq6etmtROsBWo2MBSZM1jr4DkYzMZGj1vsMJtLfpoS1Kq8ELsSdLFQDbKJXqKHl0vWxfM5SBTLdB4NAGSeaoKafUnKHgyDQ0wVLdk9mtooIAVhzFHxFPlw";
  pub(crate) const TEST_EXPONENT: &str = "AQAB";

  pub(crate) const WEB_SERVICES_ID: &str = "net.uwuwu.origa.web";
  pub(crate) const APP_ID: &str = "net.uwuwu.origa";
  pub(crate) const NONCE_HASH: &str =
    "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

  pub(crate) fn fixture_keys() -> ApplePublicKeys {
    return ApplePublicKeys {
      keys: vec![ApplePublicKey {
        kty: "RSA".to_string(),
        kid: TEST_KEY_ID.to_string(),
        key_use: "sig".to_string(),
        alg: "RS256".to_string(),
        n: TEST_MODULUS.to_string(),
        e: TEST_EXPONENT.to_string(),
      }],
    };
  }

  pub(crate) fn sign_token(claims: serde_json::Value) -> String {
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(TEST_SIGNING_PEM.as_bytes()).unwrap();
    let header = jsonwebtoken::Header {
      alg: jsonwebtoken::Algorithm::RS256,
      kid: Some(TEST_KEY_ID.to_string()),
      ..Default::default()
    };
    return jsonwebtoken::encode(&header, &claims, &key).unwrap();
  }

  pub(crate) fn valid_claims() -> serde_json::Value {
    return serde_json::json!({
      "iss": "https://appleid.apple.com",
      "aud": APP_ID,
      "exp": (chrono::Utc::now() + chrono::Duration::minutes(10)).timestamp(),
      "iat": chrono::Utc::now().timestamp(),
      "sub": "001234.abcdef.1234",
      "email": "user@privaterelay.appleid.com",
      "email_verified": true,
      "nonce": NONCE_HASH,
    });
  }
}

#[cfg(test)]
mod native_verification_tests {
  use super::test_support::*;
  use super::*;

  #[test]
  fn well_formed_native_token_verifies() {
    let claims =
      decode_id_token_with_keys(&fixture_keys(), &sign_token(valid_claims()), APP_ID).unwrap();

    assert_eq!(claims.sub, "001234.abcdef.1234");
    assert_eq!(
      claims.email.as_deref(),
      Some("user@privaterelay.appleid.com")
    );
    assert!(claims.email_verified.is_some_and(|v| v.value()));
    assert_eq!(claims.nonce.as_deref(), Some(NONCE_HASH));
  }

  #[test]
  fn web_services_id_audience_is_rejected_for_native_verification() {
    let mut claims = valid_claims();
    claims["aud"] = serde_json::json!(WEB_SERVICES_ID);

    let result = decode_id_token_with_keys(&fixture_keys(), &sign_token(claims), APP_ID);

    assert!(result.is_err());
  }

  #[test]
  fn wrong_issuer_is_rejected() {
    let mut claims = valid_claims();
    claims["iss"] = serde_json::json!("https://evil.example.com");

    let result = decode_id_token_with_keys(&fixture_keys(), &sign_token(claims), APP_ID);

    assert!(result.is_err());
  }

  #[test]
  fn expired_token_is_rejected() {
    let mut claims = valid_claims();
    claims["exp"] =
      serde_json::json!((chrono::Utc::now() - chrono::Duration::minutes(5)).timestamp());

    let result = decode_id_token_with_keys(&fixture_keys(), &sign_token(claims), APP_ID);

    assert!(result.is_err());
  }

  #[test]
  fn unknown_kid_is_rejected() {
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(TEST_SIGNING_PEM.as_bytes()).unwrap();
    let header = jsonwebtoken::Header {
      alg: jsonwebtoken::Algorithm::RS256,
      kid: Some("unknown-kid".to_string()),
      ..Default::default()
    };
    let token = jsonwebtoken::encode(&header, &valid_claims(), &key).unwrap();

    let result = decode_id_token_with_keys(&fixture_keys(), &token, APP_ID);

    assert!(matches!(result, Err(AuthError::Unauthorized)));
  }

  #[test]
  fn extract_kid_rejects_malformed_tokens() {
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    assert!(extract_kid("garbage").is_err());
    assert!(extract_kid("only.two").is_err());
    assert!(extract_kid("!!!!.bbb.ccc").is_err());
    // Well-formed base64 header without a kid field.
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
    assert!(extract_kid(&format!("{header}.bbb.ccc")).is_err());
    // Four segments are not a compact JWT.
    assert!(extract_kid(&format!("{header}.{header}.{header}.{header}")).is_err());
  }

  #[test]
  fn extract_kid_returns_header_kid() {
    let token = sign_token(valid_claims());

    assert_eq!(extract_kid(&token).unwrap(), TEST_KEY_ID);
  }
}
