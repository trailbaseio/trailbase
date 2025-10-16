/// Conversions SQLite values to JSON and back.
///
/// We entertain **two** different JSON representations, we call them "flat" and "rich".
///
/// The rich representation can be converted back and forth unambiguously. It is used for
/// SQLite <=> JS/TS bindings.
/// The flat representation requires a column type and can only be used in the context of
/// STRICT TABLES.
use base64::prelude::*;
use rusqlite::types::Value as SqliteValue;
use thiserror::Error;

use crate::sqlite::ColumnDataType;

#[derive(Debug, Error)]
pub enum JsonError {
  #[error("Float not finite")]
  Finite,
  #[error("Value not found")]
  ValueNotFound,
  #[error("Unsupported type")]
  NotSupported,
  #[error("Decoding")]
  Decode(#[from] base64::DecodeError),
  #[error("Unexpected type: {0}, expected {1:?}")]
  UnexpectedType(&'static str, ColumnDataType),
  #[error("Parse int error: {0}")]
  ParseInt(#[from] std::num::ParseIntError),
  #[error("Parse float error: {0}")]
  ParseFloat(#[from] std::num::ParseFloatError),
}

/// Convert a SQLite value to basic JSON types: String, Number, Null.
///
/// Note that this leads to both BLOBs and TEXT to end up as JSON String, which means guidance (the
/// target column type) is needed for unambiguous reverse conversion.
///
/// We use this for Record APIs.
pub fn value_to_flat_json(value: &SqliteValue) -> Result<serde_json::Value, JsonError> {
  return match value {
    SqliteValue::Null => Ok(serde_json::Value::Null),
    SqliteValue::Real(real) => match serde_json::Number::from_f64(*real) {
      Some(number) => Ok(serde_json::Value::Number(number)),
      None => Err(JsonError::Finite),
    },
    SqliteValue::Integer(integer) => Ok(serde_json::Value::Number(serde_json::Number::from(
      *integer,
    ))),
    SqliteValue::Blob(blob) => Ok(serde_json::Value::String(BASE64_URL_SAFE.encode(blob))),
    SqliteValue::Text(text) => Ok(serde_json::Value::String(text.clone())),
  };
}

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

/// Convert a SQLite value to "rich" JSON: String, Number, Null and **BLOB Objects**.
///
/// This is different from the "flat" representation above.
///
/// We use this for SQLite <=> JS/TS bindings.
pub fn value_to_rich_json(value: &SqliteValue) -> Result<serde_json::Value, JsonError> {
  return match value {
    SqliteValue::Null => Ok(serde_json::Value::Null),
    SqliteValue::Real(real) => match serde_json::Number::from_f64(*real) {
      Some(number) => Ok(serde_json::Value::Number(number)),
      None => Err(JsonError::Finite),
    },
    SqliteValue::Integer(integer) => Ok(serde_json::Value::Number(serde_json::Number::from(
      *integer,
    ))),
    SqliteValue::Blob(blob) => Ok(serde_json::json!({
        "blob": BASE64_URL_SAFE.encode(blob)
    })),
    SqliteValue::Text(text) => Ok(serde_json::Value::String(text.clone())),
  };
}

pub fn rich_json_to_value(value: serde_json::Value) -> Result<SqliteValue, JsonError> {
  return match value {
    serde_json::Value::Object(mut map) => {
      match map.remove("blob") {
        Some(serde_json::Value::String(str)) => {
          return Ok(SqliteValue::Blob(BASE64_URL_SAFE.decode(&str)?));
        }
        // NOTE: We're a bit lenient here, we will also accept int arrays as blobs.
        Some(serde_json::Value::Array(bytes)) => {
          return Ok(SqliteValue::Blob(json_array_to_bytes(&bytes)?));
        }
        _ => {}
      }

      Err(JsonError::NotSupported)
    }
    serde_json::Value::Array(_arr) => Err(JsonError::NotSupported),
    serde_json::Value::Null => Ok(SqliteValue::Null),
    serde_json::Value::Bool(b) => Ok(SqliteValue::Integer(b as i64)),
    serde_json::Value::String(str) => Ok(SqliteValue::Text(str)),
    serde_json::Value::Number(number) => {
      if let Some(n) = number.as_i64() {
        Ok(SqliteValue::Integer(n))
      } else if let Some(n) = number.as_u64() {
        Ok(SqliteValue::Integer(n as i64))
      } else if let Some(n) = number.as_f64() {
        Ok(SqliteValue::Real(n))
      } else {
        Err(JsonError::Finite)
      }
    }
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
    ColumnDataType::Blob => Ok(SqliteValue::Blob(match (value.len(), value) {
      // Special handling for text encoded UUIDs. Right now we're guessing based on length, it
      // would be more explicit rely on CHECK(...) column options.
      // NOTE: That uuids also parse as url-safe base64, that's why we treat it as a fall-first.
      (36, v) => uuid::Uuid::parse_str(&v)
        .map(|v| v.into())
        .or_else(|_| BASE64_URL_SAFE.decode(&v))?,
      (_, v) => BASE64_URL_SAFE.decode(&v)?,
    })),
    _ => Err(JsonError::UnexpectedType("string", data_type)),
  };
}

pub fn parse_string_to_sqlite_value(
  data_type: ColumnDataType,
  value: String,
) -> Result<SqliteValue, JsonError> {
  return Ok(match data_type {
    // Strict/storage types
    ColumnDataType::Any => SqliteValue::Text(value),
    ColumnDataType::Text => SqliteValue::Text(value),
    ColumnDataType::Blob => SqliteValue::Blob(match (value.len(), value) {
      // Special handling for text encoded UUIDs. Right now we're guessing based on length, it
      // would be more explicit rely on CHECK(...) column options.
      // NOTE: That uuids also parse as url-safe base64, that's why we treat it as a fall-first.
      (36, v) => uuid::Uuid::parse_str(&v)
        .map(|v| v.into())
        .or_else(|_| BASE64_URL_SAFE.decode(&v))?,
      (_, v) => BASE64_URL_SAFE.decode(&v)?,
    }),
    ColumnDataType::Integer => SqliteValue::Integer(value.parse::<i64>()?),
    ColumnDataType::Real => SqliteValue::Real(value.parse::<f64>()?),
  });
}

pub fn json_array_to_bytes(values: &[serde_json::Value]) -> Result<Vec<u8>, JsonError> {
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

    let id_string = "01950408-de17-7f13-8ef5-66d90b890bfd";
    let id = uuid::Uuid::parse_str(id_string).unwrap();

    conn
      .execute(
        "INSERT INTO test (id, text) VALUES ($1, $2);",
        params!(id.into_bytes(), "mytext",),
      )
      .await
      .unwrap();

    let value =
      strict_parse_string_to_sqlite_value(ColumnDataType::Blob, id_string.to_string()).unwrap();
    let blob = match value {
      rusqlite::types::Value::Blob(ref blob) => blob.clone(),
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
