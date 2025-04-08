use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Error;

/// File input schema used both for multipart-form uploads (in which case the name is mapped to
/// column names) and JSON where the column name is extracted from the corresponding key of the
/// parent object.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileUploadInput {
  /// The name of the form's file control.
  pub name: Option<String>,

  /// The file's file name.
  pub filename: Option<String>,

  /// The file's content type.
  pub content_type: Option<String>,

  /// The file's data
  pub data: Vec<u8>,
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
