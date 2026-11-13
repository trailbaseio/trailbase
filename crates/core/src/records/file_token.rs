use base64::prelude::*;
use jsonwebtoken::jwk::{AlgorithmParameters, Jwk, OctetKeyParameters, OctetKeyType};
use trailbase_schema::db::QualifiedNameEscaped;
use trailbase_schema::json_schema::file::FileTokenRef;
use trailbase_schema::json_schema::{
  Error as JsonError, FileToken, FileUpload, FileUploadResponse,
};

use crate::InitError;
use crate::aes::{KeyType, decrypt, encrypt, generate_random_key};
use crate::data_dir::DataDir;

type AnyError = Box<dyn std::error::Error + Sync + Send>;

/// Used on the RecordAPI read paths (read + list) to turn storage representation of FileUpload into
/// a FileUploadResponse.
pub fn build_file_metadata_sanitizer(
  key: &KeyType,
  table_name: &QualifiedNameEscaped,
  record_id: &trailbase_sqlite::Value,
) -> impl Fn(&str, FileUpload) -> Result<trailbase_schema::json::Value<'static>, AnyError> + use<> {
  let table_name = table_name.clone();
  let record_id = record_id.clone();
  let key = *key;

  return move |column_name: &str,
               metadata: FileUpload|
        -> Result<trailbase_schema::json::Value<'static>, AnyError> {
    let file_upload_response =
      sanitize_file_metadata_impl(&key, &table_name, column_name, &record_id, metadata)?;

    return Ok(trailbase_schema::json::Value::Raw(
      serde_json::value::to_raw_value(&file_upload_response)?,
    ));
  };
}

/// Takes the FileMetadata during reads and builds an opaque encrypted token and converts everything
/// into a FileUploadResponse.
#[inline]
fn sanitize_file_metadata_impl(
  key: &KeyType,
  table_name: &QualifiedNameEscaped,
  column_name: &str,
  pk: &trailbase_sqlite::Value,
  metadata: FileUpload,
) -> Result<FileUploadResponse, AnyError> {
  let token = file_token_encrypt(
    key,
    FileTokenRef {
      table_name,
      column_name,
      pk,
      metadata: &metadata,
    },
  )?;

  return Ok(FileUploadResponse {
    content_type: metadata.mime_type.or(metadata.content_type),
    original_filename: metadata.original_filename,
    filename: metadata.filename,
    token: Some(token),
  });
}

/// Used on the RecordAPI update paths (update + transactions) to turn tokens into a FileUpload
/// storage representation.
pub fn build_file_token_decoder(
  key: &KeyType,
  table_name: &QualifiedNameEscaped,
) -> impl Fn(&str, &trailbase_sqlite::Value, &str) -> Result<FileToken, JsonError> + use<> {
  let key = *key;
  let table_name = table_name.clone();

  return move |column_name: &str, pk: &trailbase_sqlite::Value, token: &str| {
    let token = file_token_decrypt(&key, &table_name, token).map_err(JsonError::Other)?;

    if token.table_name != *table_name || token.column_name != column_name || token.pk != *pk {
      return Err(JsonError::Other("invalid".into()));
    }

    return Ok(token);
  };
}

#[inline]
fn file_token_encrypt<'a>(key: &KeyType, token: FileTokenRef<'a>) -> Result<String, AnyError> {
  return Ok(BASE64_STANDARD_NO_PAD.encode(&encrypt(
    key,
    token.table_name.as_bytes(),
    &serde_json::to_vec(&token)?,
  )?));
}

#[inline]
pub(crate) fn file_token_decrypt(
  key: &KeyType,
  table_name: &QualifiedNameEscaped,
  token: &str,
) -> Result<FileToken, AnyError> {
  return Ok(serde_json::from_slice(&decrypt(
    key,
    table_name.as_bytes(),
    &BASE64_STANDARD_NO_PAD.decode(token)?,
  )?)?);
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
