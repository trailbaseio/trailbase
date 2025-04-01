use jsonschema::Validator;
use lazy_static::lazy_static;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use trailbase_extension::jsonschema::{SchemaEntry, ValidationError};
use uuid::Uuid;

#[derive(Debug, Clone, Error)]
pub enum SchemaError {
  #[error("JSONSchema validation error: {0}")]
  JsonSchema(Arc<ValidationError>),
  #[error("Cannot update builtin schemas")]
  BuiltinSchema,
  #[error("Missing name")]
  MissingName,
}

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
  pub fn consume(self) -> Result<(Option<String>, FileUpload, Vec<u8>), SchemaError> {
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

fn builtin_schemas() -> &'static HashMap<String, SchemaEntry> {
  fn validate_mime_type(value: &serde_json::Value, extra_args: Option<&str>) -> bool {
    let Some(valid_mime_types) = extra_args else {
      return true;
    };

    if let serde_json::Value::Object(ref map) = value {
      if let Some(serde_json::Value::String(mime_type)) = map.get("mime_type") {
        if valid_mime_types.contains(mime_type) {
          return true;
        }
      }
    }

    return false;
  }

  lazy_static! {
    static ref builtins: HashMap<String, SchemaEntry> = HashMap::<String, SchemaEntry>::from([
      (
        "std.FileUpload".to_string(),
        SchemaEntry::from(
          serde_json::to_value(schema_for!(FileUpload)).expect("infallible"),
          Some(Arc::new(validate_mime_type))
        )
        .expect("infallible")
      ),
      (
        "std.FileUploads".to_string(),
        SchemaEntry::from(
          serde_json::to_value(schema_for!(FileUploads)).expect("infallible"),
          None
        )
        .expect("infallible"),
      )
    ]);
  }

  return &builtins;
}

#[derive(Debug, Clone)]
pub struct Schema {
  pub name: String,
  pub schema: serde_json::Value,
  pub builtin: bool,
}

pub fn get_schema(name: &str) -> Option<Schema> {
  let builtins = builtin_schemas();

  trailbase_extension::jsonschema::get_schema(name).map(|s| Schema {
    name: name.to_string(),
    schema: s,
    builtin: builtins.contains_key(name),
  })
}

pub fn get_compiled_schema(name: &str) -> Option<Arc<Validator>> {
  trailbase_extension::jsonschema::get_compiled_schema(name)
}

pub fn get_schemas() -> Vec<Schema> {
  let builtins = builtin_schemas();
  return trailbase_extension::jsonschema::get_schemas()
    .into_iter()
    .map(|(name, value)| {
      let builtin = builtins.contains_key(&name);
      return Schema {
        name,
        schema: value,
        builtin,
      };
    })
    .collect();
}

pub fn set_user_schema(name: &str, pattern: Option<serde_json::Value>) -> Result<(), SchemaError> {
  let builtins = builtin_schemas();
  if builtins.contains_key(name) {
    return Err(SchemaError::BuiltinSchema);
  }

  if let Some(p) = pattern {
    let entry = SchemaEntry::from(p, None).map_err(|err| SchemaError::JsonSchema(Arc::new(err)))?;
    trailbase_extension::jsonschema::set_schema(name, Some(entry));
  } else {
    trailbase_extension::jsonschema::set_schema(name, None);
  }

  return Ok(());
}

lazy_static! {
  static ref INIT: parking_lot::Mutex<bool> = parking_lot::Mutex::new(false);
}

pub fn set_user_schemas(schemas: Vec<(String, serde_json::Value)>) -> Result<(), SchemaError> {
  let mut entries: Vec<(String, SchemaEntry)> = vec![];
  for (name, entry) in builtin_schemas() {
    entries.push((name.clone(), entry.clone()));
  }

  for (name, schema) in schemas {
    entries.push((
      name,
      SchemaEntry::from(schema, None).map_err(|err| SchemaError::JsonSchema(Arc::new(err)))?,
    ));
  }

  trailbase_extension::jsonschema::set_schemas(Some(entries));

  *INIT.lock() = true;

  return Ok(());
}

pub(crate) fn try_init_schemas() {
  let mut init = INIT.lock();
  if !*init {
    let entries = builtin_schemas()
      .iter()
      .map(|(name, entry)| (name.clone(), entry.clone()))
      .collect::<Vec<_>>();

    trailbase_extension::jsonschema::set_schemas(Some(entries));
    *init = true;
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn test_builtin_schemas() {
    assert!(builtin_schemas().len() > 0);

    for (name, schema) in builtin_schemas() {
      trailbase_extension::jsonschema::set_schema(&name, Some(schema.clone()));
    }

    {
      let schema = get_schema("std.FileUpload").unwrap();
      let compiled_schema = Validator::new(&schema.schema).unwrap();
      let input = json!({
        "id": "foo",
        "mime_type": "my_foo",
      });
      if let Err(err) = compiled_schema.validate(&input) {
        panic!("{err:?}");
      };
    }

    {
      let schema = get_schema("std.FileUploads").unwrap();
      let compiled_schema = Validator::new(&schema.schema).unwrap();
      assert!(compiled_schema
        .validate(&json!([
          {
            "id": "foo0",
            "mime_type": "my_foo0",
          },
          {
            "id": "foo1",
            "mime_type": "my_foo1",
          },
        ]))
        .is_ok());
    }
  }
}
