use base64::prelude::*;
use jsonwebtoken::jwk::{AlgorithmParameters, Jwk, OctetKeyParameters, OctetKeyType};
use trailbase_schema::json_schema::{FileToken, FileUpload, FileUploadResponse};

use crate::InitError;
use crate::aes::{KeyType, decrypt, encrypt, generate_random_key};
use crate::data_dir::DataDir;

fn aes256_from_jwk(json: &str) -> Result<KeyType, InitError> {
  let jwk: Jwk = serde_json::from_str(json)?;
  let AlgorithmParameters::OctetKey(oct) = &jwk.algorithm else {
    return Err(InitError::Parse("invalid jwk".into()));
  };

  let bytes = BASE64_URL_SAFE_NO_PAD.decode(&oct.value)?;
  let key: KeyType = bytes
    .as_slice()
    .try_into()
    .map_err(|err: std::array::TryFromSliceError| InitError::Parse(err.into()))?;

  return Ok(key);
}

pub(crate) fn get_or_init_aes256_file_key(data_dir: &DataDir) -> Result<KeyType, InitError> {
  let path = data_dir.secrets_path().join("file.jwk");

  let contents = match std::fs::read_to_string(&path) {
    Ok(contents) => contents,
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
      let new_key = generate_random_key();
      let jwk = AlgorithmParameters::OctetKey(OctetKeyParameters {
        key_type: OctetKeyType::Octet,
        value: BASE64_URL_SAFE_NO_PAD.encode(new_key),
      });
      let contents = serde_json::to_string(&jwk)?;

      std::fs::write(&path, &contents)?;

      contents
    }
    Err(err) => {
      return Err(err.into());
    }
  };

  return aes256_from_jwk(&contents);
}

fn file_metadata_sanitizer_impl(
  key: &KeyType,
  table_name: &str,
  column_name: &str,
  pk: &trailbase_sqlite::Value,
  metadata: FileUpload,
) -> Result<FileUploadResponse, Box<dyn std::error::Error + Sync + Send>> {
  return Ok(FileUploadResponse {
    content_type: metadata.mime_type.clone().or(metadata.content_type.clone()),
    original_filename: metadata.original_filename.clone(),
    filename: metadata.filename.clone(),
    token: serde_json::to_string(&FileToken {
      table_name: table_name.to_string(),
      column_name: column_name.to_string(),
      pk: pk.clone(),
      metadata,
    })
    .ok()
    .and_then(|token| encrypt(key, table_name.as_bytes(), token.as_bytes()).ok())
    .map(|bytes| BASE64_STANDARD_NO_PAD.encode(&bytes)),
  });
}

/// Takes the FileMetadata during reads and builds an opaque encrypted token and converts everything
/// into a FileUploadResponse.
pub fn file_metadata_sanitizer(
  key: &KeyType,
  table_name: &str,
  column_name: &str,
  pk: &trailbase_sqlite::Value,
  metadata: FileUpload,
) -> Result<trailbase_schema::json::Value<'static>, Box<dyn std::error::Error + Sync + Send>> {
  return Ok(trailbase_schema::json::Value::Raw(
    serde_json::value::to_raw_value(&file_metadata_sanitizer_impl(
      key,
      table_name,
      column_name,
      pk,
      metadata,
    )?)?,
  ));
}

pub fn file_token_decrypt(
  key: &KeyType,
  table_name: &str,
  token: &str,
) -> Result<FileToken, Box<dyn std::error::Error + Sync + Send>> {
  let json = decrypt(
    key,
    table_name.as_bytes(),
    &BASE64_STANDARD_NO_PAD.decode(token)?,
  )?;

  return Ok(serde_json::from_str(&String::from_utf8_lossy(&json))?);
}
