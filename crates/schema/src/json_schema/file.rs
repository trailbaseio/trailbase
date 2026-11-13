use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::json_schema::error::Error;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct FileToken {
  /// Name of the table, the file is from.
  pub table_name: String,
  /// Column name the file is from. Needs to be included, triggers are installed per-column,
  /// consequently moving between columns is not possible w/o the file being deleted.
  pub column_name: String,
  /// Record's primary key.
  pub pk: trailbase_sqlite::Value,
  /// Original FileUpload being round-tripped.
  pub metadata: FileUpload,
}

// Same as above but with references for effiecient serialization.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FileTokenRef<'a> {
  /// Name of the table, the file is from.
  pub table_name: &'a str,
  /// Column name the file is from. Needs to be included, triggers are installed per-column,
  /// consequently moving between columns is not possible w/o the file being deleted.
  pub column_name: &'a str,
  /// Record's primary key.
  pub pk: &'a trailbase_sqlite::Value,
  /// Original FileUpload being round-tripped.
  pub metadata: &'a FileUpload,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Payload {
  Bytes(Vec<u8>),
  Token {
    table_name: String,
    column_name: String,
    pk: trailbase_sqlite::Value,
  },
  // TODO: Remove after switching everything to tokens.
  Id,
}

pub type FileTokenDecoder<'a> = dyn Fn(&str) -> Result<FileToken, Error> + 'a;

/// Wire representation of upload requests, which may either contain data or use a token to
/// represent round-tripped data for update operations.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[cfg_attr(debug_assertions, derive(Serialize))]
#[serde(untagged)]
#[schemars(description = "")]
pub enum FileUploadRequest {
  /// Input data: metadata + bytes.
  #[schemars(description = "")]
  Data(FileUploadData),
  /// Encoded token representation.
  #[schemars(description = "")]
  Token { token: String },
}

impl FileUploadRequest {
  /// Returns (name, json, payload (token or bytes)).
  pub fn consume(
    self,
    token_decoder: Option<&FileTokenDecoder>,
  ) -> Result<(Option<String>, FileUpload, Payload), Error> {
    return match self {
      FileUploadRequest::Data(data) => data.consume(),
      FileUploadRequest::Token { token } => {
        let Some(token_decoder) = token_decoder else {
          return Err(Error::UnexpectedFileToken);
        };

        let FileToken {
          table_name,
          column_name,
          pk,
          metadata,
        } = token_decoder(&token)?;

        Ok((
          Some(column_name.clone()),
          metadata,
          Payload::Token {
            table_name,
            column_name,
            pk,
          },
        ))
      }
    };
  }
}

/// File input schema used both for multipart-form uploads (in which case the name is mapped to
/// column names) and JSON where the column name is extracted from the corresponding key of the
/// parent object.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[cfg_attr(debug_assertions, derive(Serialize))]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
#[schemars(description = "")]
pub struct FileUploadData {
  /// The name of the form's file control.
  #[schemars(description = "")]
  pub name: Option<String>,

  /// The file's original file name.
  #[schemars(description = "")]
  pub filename: Option<String>,

  /// Optional, user-provided content-type.
  #[schemars(description = "")]
  pub content_type: Option<String>,

  /// The file's actual byte data
  ///
  /// Note that we're using a custom serializer to support denser base64 encoding for JSON when
  /// Vec<u8>, would otherwise be serialized as `"data": [0, 1, 1]`.
  #[schemars(description = "", schema_with = "string_or_byte_array_schema")]
  #[serde(with = "bytes_or_base64")]
  pub data: Vec<u8>,
}

impl FileUploadData {
  /// Returns (name, json, bytes).
  pub fn consume(self) -> Result<(Option<String>, FileUpload, Payload), Error> {
    let Self {
      name,
      filename,
      content_type,
      data,
    } = self;

    // We don't trust user provided type, we check ourselves.
    let mime_type = infer::get(&data).map(|t| t.mime_type().to_string());

    return Ok((
      name,
      FileUpload::new(Uuid::new_v4(), filename, content_type, mime_type),
      Payload::Bytes(data),
    ));
  }
}

/// Wire-representation of FileUpload responses, e.g. read_file.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[cfg_attr(debug_assertions, derive(Deserialize))]
#[schemars(description = "")]
pub struct FileUploadResponse {
  /// An opaque token to reference this file in update-operations.
  #[schemars(description = "")]
  pub token: Option<String>,

  /// The file's original file name.
  #[serde(skip_serializing_if = "Option::is_none")]
  #[schemars(description = "")]
  pub original_filename: Option<String>,

  /// A unique filename derived from `original_filename`. Helps to address content caching issues
  /// with proxies, CDNs, ... .
  #[schemars(description = "")]
  pub filename: String,

  /// The file's content-type as provided by a user.
  #[serde(skip_serializing_if = "Option::is_none")]
  #[schemars(description = "")]
  pub content_type: Option<String>,
}

/// DB at-rest representation of FileUploads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileUpload {
  /// The file's text-encoded UUID from which the objectstore path is derived.
  /// the two use-cases into different representations.
  pub id: Uuid,

  /// The file's original file name.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub original_filename: Option<String>,

  /// A unique filename derived from `original_filename`. Helps to address content caching issues
  /// with proxies, CDNs, ... .
  pub filename: String,

  /// The file's content-type as provided by a user.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub content_type: Option<String>,

  /// The file's inferred mime type. Not user provided.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub mime_type: Option<String>,
}

impl FileUpload {
  pub fn new(
    id: Uuid,
    original_filename: Option<String>,
    content_type: Option<String>,
    mime_type: Option<String>,
  ) -> Self {
    return Self {
      id,
      filename: build_unique_filename(original_filename.as_deref()),
      original_filename,
      content_type,
      mime_type,
    };
  }

  pub fn objectstore_id(&self) -> &Uuid {
    debug_assert_eq!(4, self.id.get_version_num());
    return &self.id;
  }

  pub fn filename(&self) -> &str {
    return &self.filename;
  }

  pub fn content_type(&self) -> Option<&str> {
    return self.mime_type.as_deref().or(self.content_type.as_deref());
  }

  pub fn original_filename(&self) -> Option<&str> {
    return self.original_filename.as_deref();
  }
}

fn generate_random_lower_case_alphanumeric(length: usize) -> String {
  use rand::Rng;

  const GEN_ASCII_STR_CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
  const RANGE: u32 = GEN_ASCII_STR_CHARSET.len() as u32;

  let mut rng = rand::rng();
  let _: &dyn rand::CryptoRng = &rng;

  return String::from_iter(
    (0..length).map(|_| GEN_ASCII_STR_CHARSET[(rng.next_u32() % RANGE) as usize] as char),
  );
}

fn build_unique_filename(original_filename: Option<&str>) -> String {
  let rand = generate_random_lower_case_alphanumeric(10);
  let Some(original_filename) = original_filename else {
    return rand;
  };

  fn filter_char(c: char) -> bool {
    return !(c.is_alphanumeric() || ['-', '_', '.'].contains(&c));
  }

  let path = std::path::PathBuf::from(original_filename.replace(filter_char, ""));

  return match (
    path.file_stem().map(|s| s.to_string_lossy()),
    path.extension().map(|s| s.to_string_lossy()),
  ) {
    (Some(stem), Some(ext)) => format!("{stem}_{rand}.{ext}"),
    (Some(stem), None) => format!("{stem}_{rand}"),
    (None, _) => rand,
  };
}

fn string_or_byte_array_schema(g: &mut SchemaGenerator) -> Schema {
  // Construct a schema containing both possibilities
  schemars::json_schema!({
    "anyOf": [
      schemars::json_schema!({
        "type": "string",
        "contentEncoding": "base64"
      }),
      g.subschema_for::<Vec<u8>>()
    ],
  })
}

mod bytes_or_base64 {
  use base64::prelude::*;

  #[allow(unused)]
  pub fn serialize<S: serde::Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
    use serde::Serialize;

    return if s.is_human_readable() {
      String::serialize(&BASE64_URL_SAFE.encode(v), s)
    } else {
      Vec::<u8>::serialize(v, s)
    };
  }

  pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    use serde::Deserialize;

    if !d.is_human_readable() {
      return Vec::<u8>::deserialize(d);
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
      Bytes(Vec<u8>),
      Text(String),
    }

    return match Raw::deserialize(d)? {
      Raw::Bytes(b) => Ok(b),
      Raw::Text(s) => BASE64_URL_SAFE
        .decode(&s)
        .or_else(|_| BASE64_STANDARD.decode(&s))
        .map_err(|e| serde::de::Error::custom(format!("invalid base64: {e}"))),
    };
  }
}

#[cfg(test)]
mod tests {
  use base64::prelude::*;
  use serde::de::DeserializeOwned;
  use serde_json::json;
  use std::path::PathBuf;

  use super::*;

  #[test]
  fn test_build_unique_filename() {
    assert_eq!(10, build_unique_filename(None).len());

    let p0 = PathBuf::from(&build_unique_filename(Some("test.png")));
    assert_eq!("png", p0.extension().map(|p| p.to_string_lossy()).unwrap());
    assert!(
      p0.file_stem()
        .map(|p| p.to_string_lossy())
        .unwrap()
        .starts_with("test")
    );

    let p1 = PathBuf::from(&build_unique_filename(Some("test")));
    assert!(
      p1.file_stem()
        .map(|p| p.to_string_lossy())
        .unwrap()
        .starts_with("test")
    );

    let p2 = PathBuf::from(&build_unique_filename(Some(".png")));
    assert!(
      p2.file_stem()
        .unwrap()
        .to_string_lossy()
        .starts_with(".png")
    );
  }

  #[test]
  fn test_file_upload_input_serialization() {
    // Missing data.
    assert!(serde_json::from_str::<FileUploadRequest>("{}").is_err());

    fn deserialize_and_check_schema<T: JsonSchema + DeserializeOwned>(value: serde_json::Value) {
      let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap();
      jsonschema::validate(&schema, &value).unwrap();
      serde_json::from_value::<T>(value).unwrap();
    }

    let data = [0, 1];

    deserialize_and_check_schema::<FileUploadRequest>(json!({
        "data": &data,
    }));

    deserialize_and_check_schema::<FileUploadRequest>(json!({
        "data": BASE64_URL_SAFE.encode(&data),
    }));

    deserialize_and_check_schema::<FileUploadRequest>(json!({
        "name": "test",
        "content_type": "text",
        "data": &BASE64_STANDARD.encode(&data),
    }));

    deserialize_and_check_schema::<FileUploadRequest>(json!({
        "token": "opaque",
    }));
  }
}
