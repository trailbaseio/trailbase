use base64::prelude::*;
use jsonwebtoken::jwk::{AlgorithmParameters, Jwk, OctetKeyParameters, OctetKeyType};
use serde::{Deserialize, Serialize};
use trailbase_schema::json_schema::{FileUpload, FileUploadResponse};

use crate::aes::{KeyType, encrypt, generate_random_key};
use crate::data_dir::DataDir;
use crate::records::RecordError;

fn aes256_from_jwk(json: &str) -> Result<KeyType, RecordError> {
  let jwk: Jwk = serde_json::from_str(json).map_err(|err| RecordError::Internal(err.into()))?;
  let AlgorithmParameters::OctetKey(oct) = &jwk.algorithm else {
    return Err(RecordError::Internal("invalid jwk".into()));
  };

  let bytes = BASE64_URL_SAFE_NO_PAD
    .decode(&oct.value)
    .map_err(|err| RecordError::Internal(err.into()))?;
  let key: KeyType = bytes
    .as_slice()
    .try_into()
    .map_err(|err: std::array::TryFromSliceError| RecordError::Internal(err.into()))?;

  return Ok(key);
}

fn get_or_init_aes256_file_key(data_dir: &DataDir) -> Result<KeyType, RecordError> {
  let path = data_dir.secrets_path().join("file.jwk");

  let contents = match std::fs::read_to_string(&path) {
    Ok(contents) => contents,
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
      let new_key = generate_random_key();
      let jwk = AlgorithmParameters::OctetKey(OctetKeyParameters {
        key_type: OctetKeyType::Octet,
        value: BASE64_URL_SAFE_NO_PAD.encode(new_key),
      });
      let contents =
        serde_json::to_string(&jwk).map_err(|err| RecordError::Internal(err.into()))?;

      std::fs::write(&path, &contents).map_err(|err| RecordError::Internal(err.into()))?;

      contents
    }
    Err(err) => {
      return Err(RecordError::Internal(err.into()));
    }
  };

  return aes256_from_jwk(&contents);
}

#[derive(Deserialize, Serialize)]
struct FileToken {
  /// Name of the table, the file is from.
  table_name: String,
  /// Column name the file is from. Needs to be included, triggers are installed per-column,
  /// consequently moving between columns is not possible w/o the file being deleted.
  column_name: String,
  /// Record's primary key.
  pk: serde_json::Value,

  /// Original fileupload being round-tripped.
  metadata: FileUpload,
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
      pk: trailbase_schema::json::value_ref_to_flat_json_owned(pk)?,
      metadata,
    })
    .ok()
    .and_then(|token| encrypt(key, table_name.as_bytes(), token.as_bytes()).ok())
    .map(|bytes| BASE64_STANDARD_NO_PAD.encode(&bytes)),
  });
}

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
