use schemars::schema_for;
use std::sync::Arc;

pub use trailbase_extension::jsonschema::Schema;

use crate::error::Error;
use crate::file::{FileUpload, FileUploads};

pub fn override_json_schema_registry(
  schemas: Vec<(String, serde_json::Value)>,
) -> Result<(), Error> {
  let mut entries: Vec<(String, Schema)> = vec![];
  for (name, entry) in builtin_schemas() {
    entries.push((name, entry));
  }

  for (name, schema) in schemas {
    entries.push((
      name,
      Schema::from(schema, None, false).map_err(|err| Error::JsonSchema(err.into()))?,
    ));
  }

  trailbase_extension::jsonschema::set_schemas(entries, true);

  return Ok(());
}

/// Will only initialize if custom schemas have not already been initialized.
pub fn try_init_builtin_schemas() {
  trailbase_extension::jsonschema::set_schemas(builtin_schemas(), false);
}

fn builtin_schemas() -> Vec<(String, Schema)> {
  fn validate_mime_type(value: &serde_json::Value, extra_args: Option<&str>) -> bool {
    let Some(valid_mime_types) = extra_args else {
      return true;
    };

    if let serde_json::Value::Object(map) = value
      && let Some(serde_json::Value::String(mime_type)) = map.get("mime_type")
      && valid_mime_types.contains(mime_type)
    {
      return true;
    }

    return false;
  }

  return vec![
    (
      "std.FileUpload".to_string(),
      Schema::from(
        serde_json::to_value(schema_for!(FileUpload)).expect("infallible"),
        Some(Arc::new(validate_mime_type)),
        true,
      )
      .expect("infallible"),
    ),
    (
      "std.FileUploads".to_string(),
      Schema::from(
        serde_json::to_value(schema_for!(FileUploads)).expect("infallible"),
        None,
        true,
      )
      .expect("infallible"),
    ),
  ];
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn test_builtin_schemas() {
    assert!(builtin_schemas().len() > 0);

    try_init_builtin_schemas();

    let registry = trailbase_extension::jsonschema::json_schema_registry_snapshot();
    {
      let schema = registry.get_schema("std.FileUpload").unwrap();
      let input = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "filename": "foo_8435o3.png",
        "mime_type": "my_foo",
      });
      if let Err(err) = schema.validator.validate(&input) {
        panic!("{err:?}");
      };
    }

    {
      let schema = registry.get_schema("std.FileUploads").unwrap();
      assert!(
        schema
          .validator
          .validate(&json!([
            {
              "id": uuid::Uuid::new_v4().to_string(),
              "filename": "foo0_8435o3.png",
              "mime_type": "my_foo0",
            },
            {
              "id": uuid::Uuid::new_v4().to_string(),
              "filename": "foo1_xex5o3.png",
              "mime_type": "my_foo1",
            },
          ]))
          .is_ok()
      );
    }
  }
}
