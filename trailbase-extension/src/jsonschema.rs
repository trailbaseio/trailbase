use jsonschema::Validator;
use lru::LruCache;
use parking_lot::Mutex;
use sqlite_loadable::prelude::*;
use sqlite_loadable::{api, Error as SqliteError};
use std::collections::HashMap;
use std::ffi;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::LazyLock;

pub type ValidationError = jsonschema::ValidationError<'static>;

fn validation_error_into_owned(err: jsonschema::ValidationError<'_>) -> ValidationError {
  ValidationError {
    instance_path: err.instance_path.clone(),
    instance: std::borrow::Cow::Owned(err.instance.into_owned()),
    kind: err.kind,
    schema_path: err.schema_path,
  }
}

type CustomValidatorFn = Arc<dyn Fn(&serde_json::Value, Option<&str>) -> bool + Send + Sync>;

#[derive(Clone)]
pub struct SchemaEntry {
  schema: serde_json::Value,
  validator: Arc<Validator>,
  custom_validator: Option<CustomValidatorFn>,
}

impl SchemaEntry {
  pub fn from(
    schema: serde_json::Value,
    custom_validator: Option<CustomValidatorFn>,
  ) -> Result<Self, ValidationError> {
    let validator = Validator::new(&schema).map_err(|err| validation_error_into_owned(err))?;

    return Ok(Self {
      schema,
      validator: validator.into(),
      custom_validator,
    });
  }
}

static SCHEMA_REGISTRY: LazyLock<Mutex<HashMap<String, SchemaEntry>>> =
  LazyLock::new(|| Mutex::new(HashMap::<String, SchemaEntry>::new()));

#[allow(unused)]
fn cstr_to_string(ptr: *const ffi::c_char) -> String {
  assert!(!ptr.is_null());
  let cstr = unsafe { ffi::CStr::from_ptr(ptr) };
  String::from_utf8_lossy(cstr.to_bytes()).to_string()
}

pub fn set_schemas(schema_entries: Option<Vec<(String, SchemaEntry)>>) {
  let mut lock = SCHEMA_REGISTRY.lock();
  lock.clear();

  if let Some(entries) = schema_entries {
    for (name, entry) in entries {
      lock.insert(name, entry);
    }
  }
}

pub fn set_schema(name: &str, entry: Option<SchemaEntry>) {
  if let Some(entry) = entry {
    SCHEMA_REGISTRY.lock().insert(name.to_string(), entry);
  } else {
    SCHEMA_REGISTRY.lock().remove(name);
  }
}

pub fn get_schema(name: &str) -> Option<serde_json::Value> {
  SCHEMA_REGISTRY.lock().get(name).map(|s| s.schema.clone())
}

pub fn get_compiled_schema(name: &str) -> Option<Arc<Validator>> {
  SCHEMA_REGISTRY
    .lock()
    .get(name)
    .map(|s| s.validator.clone())
}

pub fn get_schemas() -> Vec<(String, serde_json::Value)> {
  SCHEMA_REGISTRY
    .lock()
    .iter()
    .map(|(name, schema)| (name.clone(), schema.schema.clone()))
    .collect()
}

fn get_text_or_null(
  values: &[*mut sqlite3_value],
  index: usize,
) -> Result<Option<&str>, SqliteError> {
  let value = values
    .get(index)
    .ok_or_else(|| SqliteError::new_message("Missing argument"))?;

  if api::value_is_null(value) {
    return Ok(None);
  }

  return Ok(Some(api::value_text(value)?));
}

fn get_text(values: &[*mut sqlite3_value], index: usize) -> Result<&str, SqliteError> {
  let value = values
    .get(index)
    .ok_or_else(|| SqliteError::new_message("Missing argument"))?;
  assert!(!api::value_is_null(value), "Got null value");
  return Ok(api::value_text(value)?);
}

pub(crate) fn jsonschema_by_name(
  context: *mut sqlite3_context,
  values: &[*mut sqlite3_value],
) -> Result<(), SqliteError> {
  let schema_name = get_text(values, 0)?;

  // Get and parse the JSON contents. If it's invalid JSON to start with, there's not much
  // we can validate.
  let Some(contents) = get_text_or_null(values, 1)? else {
    return Ok(());
  };
  let json = serde_json::from_str(contents).map_err(|err| {
    SqliteError::new_message(format!("Invalid JSON: {contents} => {err}").as_str())
  })?;

  // Then get/build the schema validator for the given pattern.
  let Some(entry) = SCHEMA_REGISTRY.lock().get(schema_name).cloned() else {
    return Err(SqliteError::new_message(format!(
      "Schema {schema_name} not found"
    )));
  };

  if !entry.validator.is_valid(&json) {
    api::result_bool(context, false);
    return Ok(());
  }

  if let Some(validator) = entry.custom_validator {
    if !validator(&json, None) {
      api::result_bool(context, false);
      return Ok(());
    }
  }

  api::result_bool(context, true);

  return Ok(());
}

pub(crate) fn jsonschema_by_name_with_extra_args(
  context: *mut sqlite3_context,
  values: &[*mut sqlite3_value],
) -> Result<(), SqliteError> {
  let schema_name = get_text(values, 0)?;
  let extra_args = get_text(values, 2)?;

  // Get and parse the JSON contents. If it's invalid JSON to start with, there's not much
  // we can validate.
  let Some(contents) = get_text_or_null(values, 1)? else {
    return Ok(());
  };
  let json = serde_json::from_str(contents).map_err(|err| {
    SqliteError::new_message(format!("Invalid JSON: {contents} => {err}").as_str())
  })?;

  // Then get/build the schema validator for the given pattern.
  let Some(entry) = SCHEMA_REGISTRY.lock().get(schema_name).cloned() else {
    return Err(SqliteError::new_message(format!(
      "Schema {schema_name} not found"
    )));
  };

  if !entry.validator.is_valid(&json) {
    api::result_bool(context, false);
    return Ok(());
  }

  if let Some(validator) = entry.custom_validator {
    if !validator(&json, Some(extra_args)) {
      api::result_bool(context, false);
      return Ok(());
    }
  }

  api::result_bool(context, true);

  return Ok(());
}

pub(crate) fn jsonschema_matches(
  context: *mut sqlite3_context,
  values: &[*mut sqlite3_value],
) -> Result<(), SqliteError> {
  type CacheType = LazyLock<Mutex<LruCache<String, Arc<Validator>>>>;
  static SCHEMA_CACHE: CacheType =
    LazyLock::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(128).unwrap())));

  // First, get and parse the JSON contents. If it's invalid JSON to start with, there's not much
  // we can validate.
  let Some(contents) = get_text_or_null(values, 1)? else {
    return Ok(());
  };
  let json = serde_json::from_str(contents).map_err(|err| {
    SqliteError::new_message(format!("Invalid JSON: '{contents}' => {err}").as_str())
  })?;

  let pattern = get_text(values, 0)?;

  // Then get/build the schema validator for the given pattern.
  let validator: Option<Arc<Validator>> = SCHEMA_CACHE.lock().get(pattern).cloned();
  let valid = match validator {
    Some(validator) => validator.is_valid(&json),
    None => {
      let schema = serde_json::from_str(pattern)
        .map_err(|err| SqliteError::new_message(format!("Invalid JSON Schema: {err}")))?;
      let validator = Validator::new(&schema)
        .map_err(|err| SqliteError::new_message(format!("Failed to compile Schema: {err}")))?;

      let valid = validator.is_valid(&json);
      SCHEMA_CACHE
        .lock()
        .put(pattern.to_string(), Arc::new(validator));
      valid
    }
  };

  api::result_bool(context, valid);

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use libsql::params;

  #[tokio::test]
  async fn test_explicit_jsonschema() {
    let conn = crate::connect().await.unwrap();

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
    conn.query(&create_table, ()).await.unwrap();

    {
      conn
        .execute(
          r#"INSERT INTO test (text0, text1) VALUES ('{"name": "foo"}', '"text"')"#,
          params!(),
        )
        .await
        .unwrap();
    }

    {
      assert!(conn
        .execute(
          r#"INSERT INTO test (text0, text1) VALUES ('{"name": "foo", "age": -5}', '"text"')"#,
          params!(),
        )
        .await
        .is_err());
    }
  }

  #[tokio::test]
  async fn test_registerd_jsonschema() {
    let conn = crate::connect().await.unwrap();

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

    set_schema(
      "name0",
      Some(
        SchemaEntry::from(
          serde_json::from_str(text0_schema).unwrap(),
          Some(Arc::new(starts_with)),
        )
        .unwrap(),
      ),
    );

    let create_table = format!(
      r#"
        CREATE TABLE test (
          text0    TEXT NOT NULL CHECK(jsonschema('name0', text0, 'prefix'))
        ) STRICT;
      "#
    );
    conn.query(&create_table, ()).await.unwrap();

    conn
      .execute(
        r#"INSERT INTO test (text0) VALUES ('{"name": "prefix_foo"}')"#,
        params!(),
      )
      .await
      .unwrap();

    assert!(conn
      .execute(
        r#"INSERT INTO test (text0) VALUES ('{"name": "WRONG_PREFIX_foo"}')"#,
        params!(),
      )
      .await
      .is_err());
  }
}
