use base64::prelude::*;
use std::collections::HashMap;
use trailbase_sqlite::Value as SqliteValue;

use crate::json::{JsonError, value_ref_to_flat_json};
use crate::metadata::{ColumnMetadata, JsonColumnMetadata};
use crate::sqlite::ColumnOption;

pub type JsonObject = serde_json::value::Map<String, serde_json::Value>;

// We have our own Value representation for JSON serialization only to reduce allocations.
#[derive(Clone, Default)]
pub enum Value<'ctx> {
  #[default]
  Null,
  Bool(bool),
  Number(serde_json::Number),
  String(std::borrow::Cow<'ctx, str>),
  Array(Vec<Value<'ctx>>),
  // Two object representation for reference data and owned.
  Object(Vec<(&'ctx str, Value<'ctx>)>),
  ObjectOwned(JsonObject),
  // Fk
  ForeignKey {
    id: serde_json::Value,
    data: Option<Box<serde_json::value::RawValue>>,
  },
  // Nested unparsed json.
  Raw(Box<serde_json::value::RawValue>),
}

impl serde::ser::Serialize for Value<'_> {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::ser::Serializer,
  {
    match self {
      Value::Null => serializer.serialize_unit(),
      Value::Bool(b) => serializer.serialize_bool(*b),
      Value::Number(n) => n.serialize(serializer),
      Value::String(s) => serializer.serialize_str(s),
      Value::Array(v) => serializer.collect_seq(v),
      Value::Object(m) => serializer.collect_map(m.iter().map(|(k, v)| (*k, v))),
      Value::ObjectOwned(m) => m.serialize(serializer),
      Value::ForeignKey { id, data } => {
        use serde::ser::SerializeMap;
        let mut s = serializer.serialize_map(Some(if data.is_some() { 2 } else { 1 }))?;
        s.serialize_entry("id", id)?;
        if let Some(data) = data {
          s.serialize_entry("data", data)?;
        }
        s.end()
      }
      Value::Raw(j) => j.serialize(serializer),
    }
  }
}

#[inline]
fn value_to_flat_json_borrow<'a>(value: &'a SqliteValue) -> Result<Value<'a>, JsonError> {
  return match value {
    SqliteValue::Null => Ok(Value::Null),
    SqliteValue::Real(f) => Ok(Value::Number(
      serde_json::Number::from_f64(*f).ok_or(JsonError::Finite)?,
    )),
    SqliteValue::Integer(integer) => Ok(Value::Number(serde_json::Number::from(*integer))),
    SqliteValue::Blob(blob) => Ok(Value::String(BASE64_URL_SAFE.encode(blob).into())),
    SqliteValue::Text(text) => Ok(Value::String(std::borrow::Cow::Borrowed(text))),
  };
}

pub trait Record {
  fn len(&self) -> usize;
  fn get_value(&self, index: usize) -> Option<(&str, &trailbase_sqlite::Value)>;
}

impl Record for trailbase_sqlite::Row {
  #[inline]
  fn len(&self) -> usize {
    return self.column_count();
  }

  #[inline]
  fn get_value(&self, index: usize) -> Option<(&str, &trailbase_sqlite::Value)> {
    let value = self.get_value(index).ok()?;
    let name = self.column_name(index)?;
    return Some((name, value));
  }
}

impl Record for &Vec<(String, trailbase_sqlite::Value)> {
  #[inline]
  fn len(&self) -> usize {
    return Vec::len(self);
  }

  #[inline]
  fn get_value(&self, index: usize) -> Option<(&str, &trailbase_sqlite::Value)> {
    return self.get(index).map(|e| (e.0.as_str(), &e.1));
  }
}

/// Serialize SQL row to json. Skips columns prefixed with "_" and can expand foreign key columns.
pub fn record_to_json_expand(
  column_metadata: &[ColumnMetadata],
  record: impl Record,
  mut expand: Option<&HashMap<String, Option<Box<serde_json::value::RawValue>>>>,
) -> Result<Box<serde_json::value::RawValue>, JsonError> {
  // Record may contain extra columns like trailing "_rowid_" or filtered columns starting with "_".
  if column_metadata.len() > record.len() {
    return Err(JsonError::ColumnMismatch);
  }

  let obj: Vec<(&str, Value)> = column_metadata
    .iter()
    .enumerate()
    .filter(|(_i, meta)| !meta.column.name.starts_with("_"))
    .map(|(i, meta)| -> Result<(&str, Value), JsonError> {
      let Some((name, value)) = record.get_value(i) else {
        return Err(JsonError::ValueNotFound);
      };

      let column = &meta.column;
      if column.name.as_str() != name {
        return Err(JsonError::ColumnMismatch);
      }

      // QUESTION: Should this go behind FK expansion? I.e. should the output be `{ fk: null }` or
      // `{ fk: {id: null} }`?
      if matches!(value, trailbase_sqlite::Value::Null) {
        return Ok((column.name.as_str(), Value::Null));
      }

      // Expand a foreign key.
      if let Some(foreign_value) = expand.as_mut().and_then(|e| e.get(&column.name)) {
        debug_assert!(is_foreign_key(&column.options));
        let id = value_ref_to_flat_json(value)?;

        return Ok((
          column.name.as_str(),
          Value::ForeignKey {
            id: id,
            data: foreign_value.clone(),
          },
        ));
      }

      // De-serialize nested JSON.
      if let trailbase_sqlite::Value::Text(str) = value
        && let Some(ref json) = meta.json
      {
        return match json {
          JsonColumnMetadata::SchemaName(x) if x == "std.FileUpload" => {
            let file_metadata: JsonObject = serde_json::from_str(str)?;
            Ok((
              column.name.as_str(),
              Value::ObjectOwned(strip_file_metadata_id(file_metadata)),
            ))
          }
          JsonColumnMetadata::SchemaName(x) if x == "std.FileUploads" => {
            let file_metadata_list: Vec<JsonObject> = serde_json::from_str(str)?;

            Ok((
              column.name.as_str(),
              Value::Array(
                file_metadata_list
                  .into_iter()
                  .map(|obj| Value::ObjectOwned(strip_file_metadata_id(obj)))
                  .collect(),
              ),
            ))
          }
          JsonColumnMetadata::SchemaName(_) | JsonColumnMetadata::Pattern(_) => Ok((
            column.name.as_str(),
            Value::Raw(serde_json::value::RawValue::from_string(str.clone())?),
          )),
        };
      }

      // De-serialize WKB Geometry.
      #[cfg(feature = "geos")]
      if meta.is_geometry {
        if let trailbase_sqlite::Value::Blob(wkb) = value {
          let geometry = geos::Geometry::new_from_wkb(wkb)?;
          let json_geometry: geos::geojson::Geometry = geometry.try_into()?;
          let obj = match serde_json::to_value(json_geometry)? {
            serde_json::Value::Object(obj) => Value::ObjectOwned(obj),
            _ => {
              panic!("should not happen");
            }
          };
          return Ok((column.name.as_str(), obj));
        }

        debug_assert!(false, "expected blob");
      }

      return Ok((column.name.as_str(), value_to_flat_json_borrow(value)?));
    })
    .collect::<Result<_, JsonError>>()?;

  return Ok(serde_json::value::to_raw_value(&Value::Object(obj)).expect("from well-formed value"));
}

#[inline]
fn strip_file_metadata_id(mut _file_metadata: JsonObject) -> JsonObject {
  #[cfg(not(test))]
  {
    _file_metadata.remove("id");
  }

  return _file_metadata;
}

#[inline]
fn is_foreign_key(options: &[ColumnOption]) -> bool {
  return options
    .iter()
    .any(|o| matches!(o, ColumnOption::ForeignKey { .. }));
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::sqlite::{Column, ColumnAffinityType, ColumnDataType};

  #[test]
  fn simple_record() {
    let column_metatada = vec![ColumnMetadata {
      index: 0,
      column: Column {
        name: "a".to_string(),
        type_name: "INTEGER".to_string(),
        data_type: ColumnDataType::Integer,
        affinity_type: ColumnAffinityType::Integer,
        options: vec![],
      },
      json: None,
      is_file: false,
      is_geometry: false,
    }];

    let record0 = vec![("a".to_string(), trailbase_sqlite::Value::Integer(5))];

    assert_eq!(
      serde_json::json!({"a": 5}),
      serde_json::from_str::<serde_json::Value>(
        &record_to_json_expand(&column_metatada, &record0, None)
          .unwrap()
          .to_string()
      )
      .unwrap(),
    );
  }
}
