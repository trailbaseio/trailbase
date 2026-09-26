use base64::prelude::*;
use trailbase_sqlite::Value as SqliteValue;

use crate::json::{JsonError, value_ref_to_flat_json};
use crate::metadata::{ColumnMetadata, JsonColumnMetadata};

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

impl<'ctx> From<Value<'ctx>> for serde_json::Value {
  fn from(value: Value<'ctx>) -> Self {
    use serde_json::Value as JValue;
    return match value {
      Value::Null => JValue::Null,
      Value::Bool(b) => JValue::Bool(b),
      Value::Number(n) => JValue::Number(n),
      Value::String(s) => JValue::String(s.to_string()),
      Value::Array(a) => JValue::Array(a.into_iter().map(|v| v.into()).collect()),
      Value::Object(o) => JValue::Object(
        o.into_iter()
          .map(|(k, v)| (k.to_string(), v.into()))
          .collect(),
      ),
      Value::ObjectOwned(o) => JValue::Object(o),
      Value::ForeignKey { id, data } => {
        if let Some(data) = data {
          serde_json::json!({
            "id": id,
            "data": serde_json::from_str::<JValue>(data.get()).expect("well-formed"),
          })
        } else {
          serde_json::json!({
            "id": id,
          })
        }
      }
      Value::Raw(raw) => serde_json::from_str(raw.get()).expect("well-formed"),
    };
  }
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

impl Record for Vec<(String, trailbase_sqlite::Value)> {
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
  expand_config: &[compact_str::CompactString],
  record: &impl Record,
  expand: Option<Vec<(compact_str::CompactString, Box<serde_json::value::RawValue>)>>,
) -> Result<Box<serde_json::value::RawValue>, JsonError> {
  return Ok(
    serde_json::value::to_raw_value(&Value::Object(record_to_json_expand_ref(
      column_metadata,
      expand_config,
      record,
      expand,
    )?))
    .expect("from well-formed value"),
  );
}

pub fn record_to_json_expand_ref<'a>(
  column_metadata: &'a [ColumnMetadata],
  expand_config: &'a [compact_str::CompactString],
  record: &'a impl Record,
  mut expand: Option<Vec<(compact_str::CompactString, Box<serde_json::value::RawValue>)>>,
) -> Result<Vec<(&'a str, Value<'a>)>, JsonError> {
  // Record may contain extra columns like trailing "_rowid_" or filtered columns starting with "_".
  if column_metadata.len() > record.len() {
    return Err(JsonError::ColumnMismatch);
  }

  return column_metadata
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
      if meta.is_fk && expand_config.iter().any(|c| *c == column.name) {
        let id = value_ref_to_flat_json(value)?;
        let Some(expand) = expand.as_mut() else {
          return Ok((
            column.name.as_str(),
            Value::ForeignKey { id: id, data: None },
          ));
        };

        return Ok((
          column.name.as_str(),
          Value::ForeignKey {
            id: id,
            data: pop_first_matching(expand, |(c, _)| *c == column.name).map(|(_, v)| v),
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
    .collect::<Result<_, JsonError>>();
}

#[cfg(feature = "geos")]
pub fn build_feature_collection(
  meta: &ColumnMetadata,
  pk_column_name: &str,
  cursor: Option<String>,
  total_count: Option<usize>,
  records: Vec<Vec<(&str, Value)>>,
) -> Result<geos::geojson::FeatureCollection, JsonError> {
  #[inline]
  fn to_json_object(record: Vec<(&str, Value)>) -> JsonObject {
    return record
      .into_iter()
      .map(|(k, v)| (k.to_string(), v.into()))
      .collect();
  }

  let foreign_members = match (cursor, total_count) {
    (Some(c), None) => Some(JsonObject::from_iter([(
      "cursor".to_string(),
      serde_json::Value::String(c),
    )])),
    (None, Some(tc)) => Some(JsonObject::from_iter([(
      "total_count".to_string(),
      serde_json::Value::Number(tc.into()),
    )])),
    (Some(c), Some(tc)) => Some(JsonObject::from_iter([
      ("cursor".to_string(), serde_json::Value::String(c)),
      (
        "total_count".to_string(),
        serde_json::Value::Number(tc.into()),
      ),
    ])),
    (None, None) => None,
  };

  let features = records
    .into_iter()
    .map(
      |mut obj: Vec<_>| -> Result<geos::geojson::Feature, JsonError> {
        let id = pop_first_matching(&mut obj, |(c, _id)| *c == pk_column_name).and_then(
          |(_c, id)| match id {
            Value::Number(n) => Some(geos::geojson::feature::Id::Number(n)),
            Value::String(s) => Some(geos::geojson::feature::Id::String(s.to_string())),
            _ => None,
          },
        );
        debug_assert!(id.is_some());

        // NOTE: Geometry may be NULL for nullable columns.
        let geometry =
          pop_first_matching(&mut obj, |(c, _v)| *c == meta.column.name).and_then(|(_c, g)| {
            return geos::geojson::Geometry::from_json_value(g.into()).ok();
          });

        return Ok(geos::geojson::Feature {
          id,
          geometry,
          properties: Some(to_json_object(obj)),
          bbox: None,
          foreign_members: None,
        });
      },
    )
    .collect::<Result<_, _>>()?;

  return Ok(geos::geojson::FeatureCollection {
    bbox: None,
    features,
    foreign_members,
  });
}

#[inline]
fn strip_file_metadata_id(mut _file_metadata: JsonObject) -> JsonObject {
  // FIXME: Our tests currently depend on the id in the response (which are in a downstream crate).
  // Enabling this in debug builds is silly. We should probably change the tests to read the id from
  // the DB instead.
  #[cfg(not(debug_assertions))]
  {
    _file_metadata.remove("id");
  }

  return _file_metadata;
}

#[inline]
fn pop_first_matching<T, F>(vec: &mut Vec<T>, predicate: F) -> Option<T>
where
  F: Fn(&T) -> bool,
{
  return Some(vec.remove(vec.iter().position(predicate)?));
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
      is_fk: false,
    }];

    let record0 = vec![("a".to_string(), trailbase_sqlite::Value::Integer(5))];

    assert_eq!(
      serde_json::json!({"a": 5}),
      serde_json::from_str::<serde_json::Value>(
        &record_to_json_expand(&column_metatada, &[], &record0, None)
          .unwrap()
          .to_string()
      )
      .unwrap(),
    );
  }
}
