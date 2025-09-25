use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Error;

/// File input schema used both for multipart-form uploads (in which case the name is mapped to
/// column names) and JSON where the column name is extracted from the corresponding key of the
/// parent object.
#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileUploadInput {
  /// The name of the form's file control.
  pub name: Option<String>,

  /// The file's original file name.
  pub filename: Option<String>,

  /// The file's content type.
  pub content_type: Option<String>,

  /// The file's actual byte data
  ///
  /// Note that we're using a custom serializer to support denser base64 encoding for JSON when
  /// Vec<u8>, would otherwise be serialized as `"data": [0, 1, 1]`.
  #[serde(with = "bytes_or_base64")]
  pub data: Vec<u8>,
}

mod bytes_or_base64 {
  use base64::prelude::*;
  use serde::{Deserialize, Serialize};
  use serde::{Deserializer, Serializer};

  // NOTE: FileUpload*Input* is serializable, this should only be used by tests, however we have no
  // means to only implement for test across crate boundaries.
  pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
    return if s.is_human_readable() {
      String::serialize(&BASE64_URL_SAFE.encode(v), s)
    } else {
      Vec::<u8>::serialize(v, s)
    };
  }

  pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    if !d.is_human_readable() {
      return Vec::<u8>::deserialize(d);
    }

    // QUESTION: Should we fall back to Vec::<u8>::deserialize on failure? Probably not.
    let str = String::deserialize(d)?;
    return BASE64_URL_SAFE
      .decode(&str)
      .or_else(|_| BASE64_STANDARD.decode(&str))
      .map_err(serde::de::Error::custom);
  }
}

impl FileUploadInput {
  pub fn consume(self) -> Result<(Option<String>, FileUpload, Vec<u8>), Error> {
    // We don't trust user provided type, we check ourselves.
    let mime_type = infer::get(&self.data).map(|t| t.mime_type().to_string());

    return Ok((
      self.name,
      FileUpload::new(
        uuid::Uuid::new_v4(),
        self.filename,
        self.content_type,
        mime_type,
      ),
      self.data,
    ));
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileUpload {
  /// The file's text-encoded UUID from which the objectstore path is derived.
  #[serde(default, skip_serializing_if = "String::is_empty")]
  id: String,

  /// The file's original file name.
  #[serde(skip_serializing_if = "Option::is_none")]
  original_filename: Option<String>,

  /// A unique filename derived from original. Helps to address content caching issues with
  /// proxies, CDNs, ... .
  filename: String,

  /// The file's user-provided content type.
  #[serde(skip_serializing_if = "Option::is_none")]
  content_type: Option<String>,

  /// The file's inferred mime type. Not user provided.
  #[serde(skip_serializing_if = "Option::is_none")]
  mime_type: Option<String>,
}

impl FileUpload {
  pub fn new(
    id: Uuid,
    original_filename: Option<String>,
    content_type: Option<String>,
    mime_type: Option<String>,
  ) -> Self {
    return Self {
      id: id.to_string(),
      filename: build_unique_filename(original_filename.as_deref()),
      original_filename,
      content_type,
      mime_type,
    };
  }

  pub fn objectstore_id(&self) -> &str {
    return &self.id;
  }

  pub fn filename(&self) -> &str {
    return &self.filename;
  }

  pub fn content_type(&self) -> Option<&str> {
    return self
      .mime_type
      .as_deref()
      .or_else(|| self.content_type.as_deref());
  }

  pub fn original_filename(&self) -> Option<&str> {
    return self.original_filename.as_deref();
  }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileUploads(pub Vec<FileUpload>);

fn generate_random_lower_case_alphanumeric(length: usize) -> String {
  use rand::RngCore;

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

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

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
}
