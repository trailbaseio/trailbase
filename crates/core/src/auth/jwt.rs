use crate::auth::user::DbUser;
use crate::rand::generate_random_string;
use crate::util::{id_to_b64, uuid_to_b64};
use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::{EncodePrivateKey, EncodePublicKey};
use ed25519_dalek::{SigningKey, VerifyingKey};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, errors::Error as JwtError};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::data_dir::DataDir;

#[derive(Debug, Error)]
pub enum JwtHelperError {
  #[error("IO error: {0}")]
  IO(#[from] std::io::Error),
  #[error("Decoding error: {0}")]
  Decode(#[from] jsonwebtoken::errors::Error),
  #[error("PKCS8 error: {0}")]
  PKCS8(#[from] ed25519_dalek::pkcs8::Error),
  #[error("PKCS8 SPKI error: {0}")]
  PKCS8Spki(#[from] ed25519_dalek::pkcs8::spki::Error),
}

#[repr(u8)]
#[allow(unused)]
pub enum TokenType {
  Unknown,
  Auth,
  PendingAuth,
  ResetPassword,
  ChangeEmail,
  VerifyEmail,
}

/// The actual "AuthToken" used for signed-in users.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthTokenClaims {
  /// Url-safe Base64 encoded id of the current user.
  pub sub: String,
  /// Unix timestamp in seconds when the token was minted.
  pub iat: i64,
  /// Expiration timestamp
  pub exp: i64,

  pub r#type: u8,

  /// Is admin user.
  #[serde(default)]
  #[serde(skip_serializing_if = "std::ops::Not::not")]
  pub admin: bool,

  // Requires multi-factor auth (MFA).
  #[serde(default)]
  #[serde(skip_serializing_if = "std::ops::Not::not")]
  pub mfa: bool,

  /// E-mail address of the [sub].
  pub email: String,

  /// CSRF random token. Requiring that the client echos this random token back on a non-cookie,
  /// non-auto-attach channel can be used to protect from CSRF.
  pub csrf_token: String,
}

impl AuthTokenClaims {
  pub(crate) fn new(db_user: &DbUser, expires_in: chrono::Duration) -> Self {
    assert!(db_user.verified);

    let now = chrono::Utc::now();
    return AuthTokenClaims {
      sub: id_to_b64(&db_user.id),
      exp: (now + expires_in).timestamp(),
      iat: now.timestamp(),
      r#type: TokenType::Auth as u8,
      admin: db_user.admin,
      mfa: db_user.totp_secret.is_some(),
      email: db_user.email.clone(),
      csrf_token: generate_random_string(20),
    };
  }

  pub fn from_auth_token(jwt: &JwtHelper, auth_token: &str) -> Result<Self, JwtError> {
    let claims = jwt.decode::<Self>(auth_token)?;
    assert_eq!(claims.r#type, TokenType::Auth as u8);
    return Ok(claims);
  }
}

#[repr(u8)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuthMethod {
  #[serde(rename = "pw")]
  Password,
  // #[serde(rename = "totp")]
  // Totp,
  // #[serde(rename = "otp")]
  // Otp,
}

// "Pending" auth token used for multi-factor auth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingAuthTokenClaims {
  /// Url-safe Base64 encoded id of the current user.
  pub sub: String,
  /// Expiration timestamp
  pub exp: i64,

  // Token type.
  pub r#type: u8,

  /// Auth method used for initiating the MFA, i.e. "first factor".
  pub method: AuthMethod,
}

impl PendingAuthTokenClaims {
  pub fn new(user_id: uuid::Uuid, expires_in: chrono::Duration) -> Self {
    let now = chrono::Utc::now();

    return Self {
      sub: uuid_to_b64(&user_id),
      exp: (now + expires_in).timestamp(),
      r#type: TokenType::PendingAuth as u8,
      method: AuthMethod::Password,
    };
  }

  pub fn from_pending_auth_token(jwt: &JwtHelper, token: &str) -> Result<Self, JwtError> {
    let claims = jwt.decode::<Self>(token)?;
    assert_eq!(claims.r#type, TokenType::PendingAuth as u8);
    return Ok(claims);
  }
}

// Password reset token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PasswordResetTokenClaims {
  /// The email of the account.
  pub sub: String,
  /// Expiration timestamp
  pub exp: i64,

  // Token type.
  pub r#type: u8,
}

impl PasswordResetTokenClaims {
  pub fn new(email: &str, expires_in: chrono::Duration) -> Self {
    let now = chrono::Utc::now();

    return Self {
      sub: email.to_string(),
      exp: (now + expires_in).timestamp(),
      r#type: TokenType::ResetPassword as u8,
    };
  }

  pub fn from_password_reset_token(jwt: &JwtHelper, token: &str) -> Result<Self, JwtError> {
    let claims = jwt.decode::<Self>(token)?;
    assert_eq!(claims.r#type, TokenType::ResetPassword as u8);
    return Ok(claims);
  }
}

pub struct JwtHelper {
  header: Header,
  validation: Validation,

  // The private key used for minting new JWTs.
  encoding_key: EncodingKey,

  // The public key used for validating provided JWTs.
  decoding_key: DecodingKey,
  public_key: String,
}

impl JwtHelper {
  pub fn new(private_key: Vec<u8>, public_key: Vec<u8>) -> Result<Self, JwtHelperError> {
    return Ok(JwtHelper {
      header: Header::new(jsonwebtoken::Algorithm::EdDSA),
      validation: Validation::new(jsonwebtoken::Algorithm::EdDSA),
      encoding_key: EncodingKey::from_ed_pem(&private_key)?,
      decoding_key: DecodingKey::from_ed_pem(&public_key)?,
      public_key: String::from_utf8_lossy(&public_key).to_string(),
    });
  }

  pub async fn init_from_path(data_dir: &DataDir) -> Result<Self, JwtHelperError> {
    let key_path = data_dir.key_path();

    async fn open_key_files(key_path: &Path) -> std::io::Result<(fs::File, fs::File)> {
      Ok((
        fs::File::open(key_path.join(PRIVATE_KEY_FILE)).await?,
        fs::File::open(key_path.join(PUBLIC_KEY_FILE)).await?,
      ))
    }

    let (private_key, public_key) = match open_key_files(&key_path).await {
      Ok((priv_key_file, pub_key_file)) => (
        read_file(priv_key_file).await?,
        read_file(pub_key_file).await?,
      ),
      Err(err) => match err.kind() {
        std::io::ErrorKind::NotFound => write_new_pem_keys(&key_path).await?,
        _ => {
          return Err(err.into());
        }
      },
    };

    return Self::new(private_key, public_key);
  }

  pub fn public_key(&self) -> String {
    return self.public_key.clone();
  }

  pub fn decode<T: DeserializeOwned + Clone>(&self, token: &str) -> Result<T, JwtError> {
    // Note: we don't need to expose the token headers.
    return jsonwebtoken::decode::<T>(token, &self.decoding_key, &self.validation)
      .map(|data| data.claims);
  }

  pub fn encode<T: Serialize>(&self, claims: &T) -> Result<String, JwtError> {
    return jsonwebtoken::encode::<T>(&self.header, claims, &self.encoding_key);
  }
}

fn generate_new_key_pair() -> (SigningKey, VerifyingKey) {
  let mut csprng = argon2::password_hash::rand_core::OsRng;
  let signing_key = SigningKey::generate(&mut csprng);
  let verifying_key = signing_key.verifying_key();

  return (signing_key, verifying_key);
}

async fn write_new_pem_keys(key_path: &Path) -> Result<(Vec<u8>, Vec<u8>), JwtHelperError> {
  let (signing_key, verifying_key) = generate_new_key_pair();

  let le = LineEnding::default();
  let priv_key = signing_key.to_pkcs8_pem(le)?.as_bytes().to_vec();
  let pub_key = verifying_key.to_public_key_pem(le)?.into_bytes();

  write_new_file(key_path.join(PRIVATE_KEY_FILE), &priv_key).await?;
  write_new_file(key_path.join(PUBLIC_KEY_FILE), &pub_key).await?;

  Ok((priv_key, pub_key))
}

async fn read_file(mut file: fs::File) -> std::io::Result<Vec<u8>> {
  let mut buffer = vec![];
  file.read_to_end(&mut buffer).await?;
  Ok(buffer)
}

async fn write_new_file(path: PathBuf, bytes: &[u8]) -> std::io::Result<()> {
  fs::File::create(&path).await?.write_all(bytes).await?;
  Ok(())
}

#[cfg(test)]
pub(crate) fn test_jwt_helper() -> JwtHelper {
  let (signing_key, verifying_key) = generate_new_key_pair();

  let private_key = signing_key
    .to_pkcs8_pem(LineEnding::default())
    .unwrap()
    .as_bytes()
    .to_vec();

  let public_key = verifying_key
    .to_public_key_pem(LineEnding::default())
    .unwrap()
    .as_bytes()
    .to_vec();

  return JwtHelper::new(private_key, public_key).unwrap();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_decode_encode() {
    let jwt = test_jwt_helper();

    let db_user = DbUser {
      id: uuid::Uuid::new_v4().into_bytes(),
      email: "foo@bar.com".to_string(),
      verified: true,
      admin: false,
      ..Default::default()
    };

    let claims = AuthTokenClaims::new(&db_user, crate::constants::DEFAULT_AUTH_TOKEN_TTL);
    let token = jwt.encode(&claims).unwrap();

    assert_eq!(claims, jwt.decode(&token).unwrap());
    assert_eq!(
      claims,
      AuthTokenClaims::from_auth_token(&jwt, &token).unwrap()
    );

    let pending_auth_claims = PendingAuthTokenClaims::new(
      uuid::Uuid::now_v7(),
      crate::constants::DEFAULT_MFA_TOKEN_TTL,
    );
    let pending_auth_token = jwt.encode(&pending_auth_claims).unwrap();

    assert_eq!(
      pending_auth_claims,
      PendingAuthTokenClaims::from_pending_auth_token(&jwt, &pending_auth_token).unwrap()
    );
    assert!(AuthTokenClaims::from_auth_token(&jwt, &pending_auth_token).is_err())
  }
}

const PRIVATE_KEY_FILE: &str = "private_key.pem";
const PUBLIC_KEY_FILE: &str = "public_key.pem";
