use crate::rand::generate_random_string;
use crate::util::uuid_to_b64;
use argon2::password_hash::rand_core::OsRng;
use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::{EncodePrivateKey, EncodePublicKey};
use ed25519_dalek::{SigningKey, VerifyingKey};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, errors::Error as JwtError};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::{
  fs,
  io::{AsyncReadExt, AsyncWriteExt},
};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenClaims {
  /// Url-safe Base64 encoded id of the current user.
  pub sub: String,
  /// Unix timestamp in seconds when the token was minted.
  pub iat: i64,
  /// Expiration timestamp
  pub exp: i64,

  /// E-mail address of the [sub].
  pub email: String,

  /// CSRF random token. Requiring that the client echos this random token back on a non-cookie,
  /// non-auto-attach channel can be used to protect from CSRF.
  pub csrf_token: String,

  /// Is admin user.
  #[serde(default)]
  #[serde(skip_serializing_if = "std::ops::Not::not")]
  pub admin: bool,
}

impl TokenClaims {
  pub fn new(
    verified: bool,
    user_id: uuid::Uuid,
    email: String,
    admin: bool,
    expires_in: chrono::Duration,
  ) -> Self {
    assert!(verified);

    let now = chrono::Utc::now();
    return TokenClaims {
      sub: uuid_to_b64(&user_id),
      exp: (now + expires_in).timestamp(),
      iat: now.timestamp(),
      email,
      csrf_token: generate_random_string(20),
      admin,
    };
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
  let mut csprng = OsRng {};
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

    let claims = TokenClaims::new(
      true,
      uuid::Uuid::now_v7(),
      "foo@bar.com".to_string(),
      false,
      crate::constants::DEFAULT_AUTH_TOKEN_TTL,
    );
    let token = jwt.encode(&claims).unwrap();

    assert_eq!(claims, jwt.decode(&token).unwrap());
  }
}

const PRIVATE_KEY_FILE: &str = "private_key.pem";
const PUBLIC_KEY_FILE: &str = "public_key.pem";
