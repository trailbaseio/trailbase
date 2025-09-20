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

  /// The file's file name.
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
      String::serialize(&BASE64_URL_SAFE.encode(&v), s)
    } else {
      Vec::<u8>::serialize(&v, s)
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
      .map_err(|e| serde::de::Error::custom(e));
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileUpload {
  /// The file's unique id from which the objectstore path is derived.
  id: String,

  /// The file's original file name.
  filename: Option<String>,

  /// The file's user-provided content type.
  content_type: Option<String>,

  /// The file's inferred mime type. Not user provided.
  mime_type: Option<String>,
}

impl FileUpload {
  pub fn new(
    id: Uuid,
    filename: Option<String>,
    content_type: Option<String>,
    mime_type: Option<String>,
  ) -> Self {
    Self {
      id: id.to_string(),
      filename,
      content_type,
      mime_type,
    }
  }

  pub fn path(&self) -> &str {
    &self.id
  }

  pub fn content_type(&self) -> Option<&str> {
    self.content_type.as_deref()
  }

  pub fn original_filename(&self) -> Option<&str> {
    self.filename.as_deref()
  }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FileUploads(pub Vec<FileUpload>);
