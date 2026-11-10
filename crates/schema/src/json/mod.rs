/// Conversions SQLite values to JSON and back.
mod error;
mod record;
mod value;

use base64::prelude::*;
use trailbase_sqlite::Value as SqliteValue;

use crate::db::sqlite::ColumnDataType;

pub use error::JsonError;
pub use record::{
  FileMetadataSanitizer, Record, record_to_json_expand, record_to_json_expand_ref,
  value_ref_to_flat_json_owned,
};
pub use value::{JsonObject, Value};

#[cfg(feature = "geos")]
pub use record::build_feature_collection;

pub fn flat_json_to_value(
  col_type: ColumnDataType,
  value: serde_json::Value,
) -> Result<SqliteValue, JsonError> {
  return match value {
    serde_json::Value::Object(ref _map) => Err(JsonError::UnexpectedType("Object", col_type)),
    serde_json::Value::Array(ref arr) => {
      // NOTE: Convert Array<number> to Blob. Note, we also support blobs as base64 which are
      // handled below in the string  case.
      match col_type {
        ColumnDataType::Blob | ColumnDataType::Any => {
          Ok(SqliteValue::Blob(json_array_to_bytes(arr)?))
        }
        _ => Err(JsonError::UnexpectedType("Array", col_type)),
      }
    }
    serde_json::Value::Null => Ok(SqliteValue::Null),
    serde_json::Value::Bool(b) => {
      match col_type.is_integer_kind() || col_type == ColumnDataType::Any {
        true => Ok(SqliteValue::Integer(b as i64)),
        false => Err(JsonError::UnexpectedType("Bool", col_type)),
      }
    }
    serde_json::Value::String(str) => strict_parse_string_to_sqlite_value(col_type, str),
    serde_json::Value::Number(number) => {
      if let Some(n) = number.as_i64() {
        if col_type.is_integer_kind() || col_type == ColumnDataType::Any {
          Ok(SqliteValue::Integer(n))
        } else if col_type.is_float_kind() {
          // NOTE: "as" is lossy conversion. Does not panic.
          Ok(SqliteValue::Real(n as f64))
        } else {
          Err(JsonError::UnexpectedType("int", col_type))
        }
      } else if let Some(n) = number.as_u64() {
        // NOTE: "as" is lossy conversion. Does not panic.
        if col_type.is_integer_kind() || col_type == ColumnDataType::Any {
          Ok(SqliteValue::Integer(n as i64))
        } else if col_type.is_float_kind() {
          Ok(SqliteValue::Real(n as f64))
        } else {
          Err(JsonError::UnexpectedType("uint", col_type))
        }
      } else if let Some(n) = number.as_f64() {
        match col_type.is_float_kind() || col_type == ColumnDataType::Any {
          true => Ok(SqliteValue::Real(n)),
          _ => Err(JsonError::UnexpectedType("real", col_type)),
        }
      } else {
        #[cfg(not(debug_assertions))]
        return Err(JsonError::Finite);

        // NOTE: It's not quite as tricial. serde_json will behave differently whether
        // its "arbitrary_precision" feature is enabled or not.
        #[cfg(debug_assertions)]
        panic!("we exhaustively checked for int, uint and float");
      }
    }
  };
}

#[inline]
pub fn parse_string_to_sqlite_value(
  data_type: ColumnDataType,
  value: String,
) -> Result<SqliteValue, JsonError> {
  return match data_type {
    // Strict/storage types
    ColumnDataType::Text | ColumnDataType::Any => Ok(SqliteValue::Text(value)),
    // Special handling for text encoded UUIDs. Right now we're guessing based on length, it
    // would be more explicit rely on CHECK(...) column options.
    ColumnDataType::Blob => match crate::parse_uuid_vec(&value) {
      Ok(uuid) => Ok(SqliteValue::Blob(uuid)),
      Err(_) => Ok(SqliteValue::Blob(BASE64_URL_SAFE.decode(&value)?)),
    },
    ColumnDataType::Integer => Ok(SqliteValue::Integer(value.parse::<i64>()?)),
    ColumnDataType::Real => Ok(SqliteValue::Real(value.parse::<f64>()?)),
  };
}

/// Strictly parse string to SqliteValue, i.e. w/o trying to parse e.g. strings into INT or REAL.
#[inline]
fn strict_parse_string_to_sqlite_value(
  data_type: ColumnDataType,
  value: String,
) -> Result<SqliteValue, JsonError> {
  return match data_type {
    ColumnDataType::Text | ColumnDataType::Any => Ok(SqliteValue::Text(value)),
    // Special handling for text encoded UUIDs. Right now we're guessing based on length, it
    // would be more explicit rely on CHECK(...) column options.
    ColumnDataType::Blob => match crate::parse_uuid_vec(&value) {
      Ok(uuid) => Ok(SqliteValue::Blob(uuid)),
      Err(_) => Ok(SqliteValue::Blob(BASE64_URL_SAFE.decode(&value)?)),
    },
    _ => Err(JsonError::UnexpectedType("string", data_type)),
  };
}

fn json_array_to_bytes(values: &[serde_json::Value]) -> Result<Vec<u8>, JsonError> {
  return values
    .iter()
    .map(|v| -> Result<u8, JsonError> {
      v.as_u64()
        .and_then(|v| u8::try_from(v).ok())
        .ok_or(JsonError::NotSupported)
    })
    .collect();
}

#[cfg(test)]
mod tests {
  use super::*;

  use trailbase_sqlite::{Connection, params};

  #[tokio::test]
  async fn test_parse_string_json_value() {
    let conn = Connection::open_in_memory().unwrap();
    conn
      .execute("CREATE TABLE test (id BLOB NOT NULL, text TEXT)", ())
      .await
      .unwrap();

    let id = uuid::Uuid::new_v4();

    conn
      .execute(
        "INSERT INTO test (id, text) VALUES ($1, $2);",
        params!(id.into_bytes(), "mytext",),
      )
      .await
      .unwrap();

    let value = strict_parse_string_to_sqlite_value(ColumnDataType::Blob, id.to_string()).unwrap();
    let blob = match value {
      trailbase_sqlite::Value::Blob(ref blob) => blob.clone(),
      _ => panic!("Not a blob"),
    };

    assert_eq!(
      blob.len(),
      16,
      "Blob: {value:?} {}",
      String::from_utf8_lossy(&blob)
    );
    assert_eq!(uuid::Uuid::from_slice(&blob).unwrap(), id);

    let rows = conn
      .read_query_rows("SELECT * FROM test WHERE id = $1", [value])
      .await
      .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String>(1).unwrap(), "mytext");
  }
}
