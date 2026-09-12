use serde::Deserialize;

use crate::auth::AuthError;

/// RFC: https://www.rfc-editor.org/info/rfc7517/#section-4
#[allow(unused)]
#[derive(Debug, Deserialize)]
struct Jwk {
  kty: String,
  kid: String,
  r#use: String,
  alg: String,
  n: String,
  e: String,
}

#[derive(Debug, Deserialize)]
pub struct ApplePublicKeys {
  keys: Vec<Jwk>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Boolean {
  String(String),
  Bool(bool),
}

impl Boolean {
  pub fn value(&self) -> bool {
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

/// Extracts the `kid` header from a JWT without any network access.
pub fn extract_kid(id_token: &str) -> Result<String, AuthError> {
  let Ok(header) = jsonwebtoken::decode_header(id_token) else {
    return Err(AuthError::BadRequest("malformed identity token"));
  };

  if let Some(kid) = header.kid
    && !kid.is_empty()
  {
    return Ok(kid);
  }

  return Err(AuthError::BadRequest(
    "identity token is missing the kid header",
  ));
}

/// Verifies signature and claims (issuer, audience, expiry) of an Apple
/// identity token against the given public keys, selecting the key by `kid`.
pub(crate) fn decode_id_token_with_keys(
  public_keys: &ApplePublicKeys,
  id_token: &str,
  audience: &str,
) -> Result<AppleIdToken, AuthError> {
  let kid = extract_kid(id_token)?;

  // Find the key.
  let Some(public_key) = public_keys.keys.iter().find(|key| key.kid == kid) else {
    return Err(AuthError::Unauthorized);
  };

  let decoding_key = jsonwebtoken::DecodingKey::from_rsa_components(&public_key.n, &public_key.e)
    .map_err(|err| {
    // Got invalid key from Apple?
    return AuthError::FailedDependency(err.into());
  })?;

  let validation = {
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_audience(&[audience]);
    validation.set_issuer(&["https://appleid.apple.com"]);
    validation
  };

  let token_data = jsonwebtoken::decode::<AppleIdToken>(id_token, &decoding_key, &validation)
    .map_err(|_err| {
      // Validation failed.
      return AuthError::Unauthorized;
    })?;

  return Ok(token_data.claims);
}

// TODO: Should maybe cache the Jwk responses.
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

/// Fixture shared by the native-verification tests here and the endpoint
/// tests in `apple_native.rs`.
#[cfg(test)]
pub mod test_support {
  use base64::prelude::*;
  use rsa::RsaPrivateKey;
  use rsa::pkcs8::EncodePrivateKey;
  use rsa::traits::PublicKeyParts;
  use std::sync::LazyLock;

  use super::*;

  /// RSA private key standing in for Apple's signing key.
  fn signing_key() -> &'static RsaPrivateKey {
    static KEY: LazyLock<RsaPrivateKey> = LazyLock::new(|| {
      let mut rng = rand::rng();
      return RsaPrivateKey::new(&mut rng, 2048).unwrap();
    });

    return &KEY;
  }

  pub fn signing_jwt_key() -> jsonwebtoken::EncodingKey {
    let pem = signing_key().to_pkcs8_pem(Default::default()).unwrap();
    return jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();
  }

  pub const TEST_KEY_ID: &str = "test-apple-key-1";
  pub const WEB_SERVICES_ID: &str = "net.uwuwu.origa.web";
  pub const APP_ID: &str = "net.uwuwu.origa";
  pub const NONCE_HASH: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

  pub fn fixture_keys() -> ApplePublicKeys {
    let key = signing_key();
    let apple_key = Jwk {
      kty: "RSA".to_string(),
      kid: TEST_KEY_ID.to_string(),
      r#use: "sig".to_string(),
      alg: "RS256".to_string(),
      n: BASE64_URL_SAFE_NO_PAD.encode(key.n().to_be_bytes()),
      e: BASE64_URL_SAFE_NO_PAD.encode(key.e().to_be_bytes()),
    };

    return ApplePublicKeys {
      keys: vec![apple_key],
    };
  }

  pub fn sign_token(claims: serde_json::Value) -> String {
    let key = signing_jwt_key();
    let header = jsonwebtoken::Header {
      alg: jsonwebtoken::Algorithm::RS256,
      kid: Some(TEST_KEY_ID.to_string()),
      ..Default::default()
    };
    return jsonwebtoken::encode(&header, &claims, &key).unwrap();
  }

  pub fn valid_claims() -> serde_json::Value {
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
mod tests {
  use base64::prelude::*;

  use super::test_support::*;
  use super::*;

  #[test]
  fn test_apple_boolean() {
    use serde_json::{from_value, json};

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
    let key = signing_jwt_key();
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
  fn test_extract_kid() {
    assert!(extract_kid("garbage").is_err());
    // Well-formed base64 header without a kid field.
    let header = BASE64_URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
    assert!(extract_kid(&format!("{header}.bbb.ccc")).is_err());

    let token = sign_token(valid_claims());
    assert_eq!(extract_kid(&token).unwrap(), TEST_KEY_ID);
  }
}
