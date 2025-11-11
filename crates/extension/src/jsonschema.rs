use jsonschema::Validator;
use mini_moka::sync::Cache;
use rusqlite::Error;
use rusqlite::functions::Context;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

// NOTE:: Validation error is very large, we thus Box it.
pub type ValidationError = Box<jsonschema::ValidationError<'static>>;

type CustomValidatorFn = Arc<dyn Fn(&serde_json::Value, Option<&str>) -> bool + Send + Sync>;

#[derive(Clone)]
pub struct Schema {
  /// The original JSON schema.
  pub schema: serde_json::Value,
  /// The precompiled validator.
  pub validator: Arc<Validator>,
  /// Marker whether this is a custom schema or a builtin provided by TB.
  pub builtin: bool,

  /// Custom validator, can be used to pass extra arguments e.g. limit file mime types.
  custom_validator: Option<CustomValidatorFn>,
}

impl Schema {
  pub fn from(
    schema: serde_json::Value,
    custom_validator: Option<CustomValidatorFn>,
    builtin: bool,
  ) -> Result<Self, ValidationError> {
    let validator = Validator::new(&schema)?;

    return Ok(Self {
      schema,
      validator: validator.into(),
      builtin,
      custom_validator,
    });
  }
}

// #[derive(Default)]
pub struct JsonSchemaRegistry {
  schemas: HashMap<String, Schema>,
}

impl JsonSchemaRegistry {
  pub fn from_schemas(schemas: Vec<(String, Schema)>) -> Self {
    return Self {
      schemas: schemas.into_iter().collect(),
    };
  }

  pub fn swap(&mut self, other: JsonSchemaRegistry) {
    self.schemas = other.schemas;
  }

  pub fn names(&self) -> Vec<String> {
    return self.schemas.keys().cloned().collect();
  }

  pub fn get_schema(&self, name: &str) -> Option<&Schema> {
    return self.schemas.get(name);
  }

  pub fn entries(&self) -> Vec<(&String, &Schema)> {
    return self.schemas.iter().collect();
  }
}

pub(super) fn jsonschema_by_name_impl(
  context: &Context,
  registry: &JsonSchemaRegistry,
) -> Result<bool, Error> {
  let schema_name = context.get_raw(0).as_str()?;

  // Get and parse the JSON contents. If it's invalid JSON to start with, there's not much
  // we can validate.
  let Some(contents) = context.get_raw(1).as_str_or_null()? else {
    return Ok(true);
  };

  let json = serde_json::from_str(contents)
    .map_err(|err| Error::UserFunctionError(format!("Invalid JSON: {contents} => {err}").into()))?;

  // Then get/build the schema validator for the given pattern.
  let Some(entry) = registry.schemas.get(schema_name) else {
    return Err(Error::UserFunctionError(
      format!("Schema {schema_name} not found").into(),
    ));
  };

  if !entry.validator.is_valid(&json) {
    return Ok(false);
  }

  if let Some(ref validator) = entry.custom_validator
    && !validator(&json, None)
  {
    return Ok(false);
  }

  return Ok(true);
}

pub(super) fn jsonschema_by_name_with_extra_args_impl(
  context: &Context,
  registry: &JsonSchemaRegistry,
) -> Result<bool, Error> {
  let schema_name = context.get_raw(0).as_str()?;
  let extra_args = context.get_raw(2).as_str()?;

  // Get and parse the JSON contents. If it's invalid JSON to start with, there's not much
  // we can validate.
  let Some(contents) = context.get_raw(1).as_str_or_null()? else {
    return Ok(true);
  };
  let json = serde_json::from_str(contents)
    .map_err(|err| Error::UserFunctionError(format!("Invalid JSON: {contents} => {err}").into()))?;

  // Then get/build the schema validator for the given pattern.
  let Some(entry) = registry.schemas.get(schema_name) else {
    return Err(Error::UserFunctionError(
      format!("Schema {schema_name} not found").into(),
    ));
  };

  if !entry.validator.is_valid(&json) {
    return Ok(false);
  }

  if let Some(ref validator) = entry.custom_validator
    && !validator(&json, Some(extra_args))
  {
    return Ok(false);
  }

  return Ok(true);
}

/// Cache for json schemas specified in CHECK(jsonschema_matches(...)).
static SCHEMA_CACHE: LazyLock<Cache<String, Arc<Validator>>> = LazyLock::new(|| Cache::new(256));

pub(crate) fn jsonschema_matches(context: &Context) -> Result<bool, Error> {
  // First, get and parse the JSON contents. If it's invalid JSON to start with, there's not much
  // we can validate.
  let Some(contents) = context.get_raw(1).as_str_or_null()? else {
    return Ok(true);
  };
  let json = serde_json::from_str(contents).map_err(|err| {
    Error::UserFunctionError(format!("Invalid JSON: '{contents}' => {err}").into())
  })?;

  let pattern = context.get_raw(0).as_str()?.to_string();

  // Then get/build the schema validator for the given pattern.
  let valid = match SCHEMA_CACHE.get(&pattern) {
    Some(validator) => validator.is_valid(&json),
    None => {
      let schema = serde_json::from_str(&pattern)
        .map_err(|err| Error::UserFunctionError(format!("Invalid JSON Schema: {err}").into()))?;
      let validator = Validator::new(&schema).map_err(|err| {
        Error::UserFunctionError(format!("Failed to compile Schema: {err}").into())
      })?;

      let valid = validator.is_valid(&json);
      SCHEMA_CACHE.insert(pattern, Arc::new(validator));
      valid
    }
  };

  return Ok(valid);
}

#[cfg(test)]
mod tests {
  use super::*;

  use parking_lot::RwLock;
  use rusqlite::params;

  #[test]
  fn test_explicit_jsonschema() {
    let conn = crate::connect_sqlite(None, None).unwrap();

    let text0_schema = r#"
        {
          "type": "object",
          "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer", "minimum": 0 }
          },
          "required": ["name"]
        }
    "#;

    let text1_schema = r#"{ "type": "string" }"#;

    let create_table = format!(
      r#"
        CREATE TABLE test (
          text0    TEXT NOT NULL CHECK(jsonschema_matches('{text0_schema}', text0)),
          text1    TEXT NOT NULL CHECK(jsonschema_matches('{text1_schema}', text1))
        ) STRICT;
      "#
    );
    conn.execute(&create_table, ()).unwrap();

    {
      conn
        .execute(
          r#"INSERT INTO test (text0, text1) VALUES ('{"name": "foo"}', '"text"')"#,
          params!(),
        )
        .unwrap();
    }

    {
      assert!(
        conn
          .execute(
            r#"INSERT INTO test (text0, text1) VALUES ('{"name": "foo", "age": -5}', '"text"')"#,
            params!(),
          )
          .is_err()
      );
    }
  }

  #[test]
  fn test_registerd_jsonschema() {
    let text0_schema = r#"
        {
          "type": "object",
          "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer", "minimum": 0 }
          },
          "required": ["name"]
        }
    "#;

    let json_schema_registry = Arc::new(RwLock::new(JsonSchemaRegistry::from_schemas(vec![(
      "name0".to_string(),
      Schema::from(
        serde_json::from_str(text0_schema).unwrap(),
        Some(Arc::new(starts_with)),
        false,
      )
      .unwrap(),
    )])));

    let conn = crate::connect_sqlite(None, Some(json_schema_registry)).unwrap();

    fn starts_with(v: &serde_json::Value, param: Option<&str>) -> bool {
      if let Some(param) = param {
        if let serde_json::Value::Object(map) = v {
          if let Some(serde_json::Value::String(str)) = map.get("name") {
            if str.starts_with(param) {
              return true;
            }
          }
        }
      }
      return false;
    }

    let create_table = format!(
      r#"
        CREATE TABLE test (
          text0    TEXT NOT NULL CHECK(jsonschema('name0', text0, 'prefix'))
        ) STRICT;
      "#
    );
    conn.execute(&create_table, ()).unwrap();

    conn
      .execute(
        r#"INSERT INTO test (text0) VALUES ('{"name": "prefix_foo"}')"#,
        params!(),
      )
      .unwrap();

    assert!(
      conn
        .execute(
          r#"INSERT INTO test (text0) VALUES ('{"name": "WRONG_PREFIX_foo"}')"#,
          params!(),
        )
        .is_err()
    );
  }
}
