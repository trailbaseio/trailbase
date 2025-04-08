use base64::prelude::*;
use log::*;
use std::collections::HashMap;
use thiserror::Error;
use trailbase_schema::sqlite::ColumnOption;
use trailbase_schema::sqlite::{Column, ColumnDataType};

use crate::table_metadata::JsonColumnMetadata;

#[derive(Debug, Error)]
pub enum JsonError {
  #[error("SerdeJson error: {0}")]
  SerdeJson(#[from] serde_json::Error),
  #[error("Malformed bytes, len {0}")]
  MalformedBytes(usize),
  #[error("Row not found")]
  RowNotFound,
  #[error("Float not finite")]
  Finite,
  #[error("Value not found")]
  ValueNotFound,
  #[error("Missing col name")]
  MissingColumnName,
}

pub(crate) fn valueref_to_json(
  value: rusqlite::types::ValueRef<'_>,
) -> Result<serde_json::Value, JsonError> {
  use rusqlite::types::ValueRef;

  return Ok(match value {
    ValueRef::Null => serde_json::Value::Null,
    ValueRef::Real(real) => {
      let Some(number) = serde_json::Number::from_f64(real) else {
        return Err(JsonError::Finite);
      };
      serde_json::Value::Number(number)
    }
    ValueRef::Integer(integer) => serde_json::Value::Number(serde_json::Number::from(integer)),
    ValueRef::Blob(blob) => serde_json::Value::String(BASE64_URL_SAFE.encode(blob)),
    ValueRef::Text(text) => serde_json::Value::String(String::from_utf8_lossy(text).to_string()),
  });
}

/// Serialize SQL row to json.
pub fn row_to_json(
  columns: &[Column],
  json_metadata: &[Option<JsonColumnMetadata>],
  row: &trailbase_sqlite::Row,
  column_filter: fn(&str) -> bool,
) -> Result<serde_json::Value, JsonError> {
  return row_to_json_expand(columns, json_metadata, row, column_filter, None);
}

#[inline]
fn is_foreign_key(options: &[ColumnOption]) -> bool {
  return options
    .iter()
    .any(|o| matches!(o, ColumnOption::ForeignKey { .. }));
}

/// Serialize SQL row to json.
pub fn row_to_json_expand(
  columns: &[Column],
  json_metadata: &[Option<JsonColumnMetadata>],
  row: &trailbase_sqlite::Row,
  column_filter: fn(&str) -> bool,
  expand: Option<&HashMap<String, serde_json::Value>>,
) -> Result<serde_json::Value, JsonError> {
  let map = (0..row.column_count())
    .filter_map(|i| {
      let Some(column_name) = row.column_name(i) else {
        return Some(Err(JsonError::MissingColumnName));
      };
      if !column_filter(column_name) {
        return None;
      }

      assert!(i < columns.len());
      assert!(i < json_metadata.len());
      let column = &columns[i];
      assert_eq!(column_name, column.name);

      let Some(value) = row.get_value(i) else {
        return Some(Err(JsonError::ValueNotFound));
      };

      if matches!(value, rusqlite::types::Value::Null) {
        return Some(Ok((column_name.to_string(), serde_json::Value::Null)));
      }

      if let Some(foreign_value) = expand.and_then(|e| e.get(column_name)) {
        if is_foreign_key(&column.options) {
          let id = match valueref_to_json(value.into()) {
            Ok(value) => value,
            Err(err) => {
              return Some(Err(err));
            }
          };

          return Some(Ok(match foreign_value {
            serde_json::Value::Null => (
              column_name.to_string(),
              serde_json::json!({
                "id": id,
              }),
            ),
            value => (
              column_name.to_string(),
              serde_json::json!({
                "id": id,
                "data": value,
              }),
            ),
          }));
        }
      }

      if let rusqlite::types::Value::Text(str) = &value {
        let metadata = &json_metadata[i];
        if metadata.is_some() {
          return match serde_json::from_str(str) {
            Ok(json) => Some(Ok((column_name.to_string(), json))),
            Err(err) => Some(Err(err.into())),
          };
        }
      }

      return match valueref_to_json(value.into()) {
        Ok(value) => Some(Ok((column_name.to_string(), value))),
        Err(err) => Some(Err(err)),
      };
    })
    .collect::<Result<serde_json::Map<_, _>, JsonError>>()?;

  return Ok(serde_json::Value::Object(map));
}

/// Turns rows into a list of json objects.
pub fn rows_to_json(
  columns: &[Column],
  json_metadata: &[Option<JsonColumnMetadata>],
  rows: trailbase_sqlite::Rows,
  column_filter: fn(&str) -> bool,
) -> Result<Vec<serde_json::Value>, JsonError> {
  return rows
    .iter()
    .map(|row| row_to_json_expand(columns, json_metadata, row, column_filter, None))
    .collect::<Result<Vec<_>, JsonError>>();
}

/// Turns rows into a list of json objects.
pub fn rows_to_json_expand(
  columns: &[Column],
  json_metadata: &[Option<JsonColumnMetadata>],
  rows: trailbase_sqlite::Rows,
  column_filter: fn(&str) -> bool,
  expand: Option<&HashMap<String, serde_json::Value>>,
) -> Result<Vec<serde_json::Value>, JsonError> {
  return rows
    .iter()
    .map(|row| row_to_json_expand(columns, json_metadata, row, column_filter, expand))
    .collect::<Result<Vec<_>, JsonError>>();
}

pub fn row_to_json_array(row: &trailbase_sqlite::Row) -> Result<Vec<serde_json::Value>, JsonError> {
  let cols = row.column_count();
  let mut json_row = Vec::<serde_json::Value>::with_capacity(cols);

  for i in 0..cols {
    let value = row.get_value(i).ok_or(JsonError::ValueNotFound)?;
    json_row.push(valueref_to_json(value.into())?);
  }

  return Ok(json_row);
}

/// Best-effort conversion from row values to column definition.
///
/// WARN: This is lossy and whenever possible we should rely on parsed "CREATE TABLE" statement for
/// the respective column.
fn rows_to_columns(rows: &trailbase_sqlite::Rows) -> Result<Vec<Column>, rusqlite::Error> {
  use trailbase_sqlite::ValueType as T;

  let mut columns: Vec<Column> = vec![];
  for i in 0..rows.column_count() {
    columns.push(Column {
      name: rows.column_name(i).unwrap_or("<missing>").to_string(),
      data_type: match rows.column_type(i).unwrap_or(T::Null) {
        T::Real => ColumnDataType::Real,
        T::Text => ColumnDataType::Text,
        T::Integer => ColumnDataType::Integer,
        T::Null => ColumnDataType::Null,
        T::Blob => ColumnDataType::Blob,
      },
      // We cannot derive the options from a row of data.
      options: vec![],
    });
  }

  return Ok(columns);
}

type Row = Vec<serde_json::Value>;

pub fn rows_to_json_arrays(
  rows: trailbase_sqlite::Rows,
  limit: usize,
) -> Result<(Vec<Row>, Option<Vec<Column>>), JsonError> {
  let columns = match rows_to_columns(&rows) {
    Ok(columns) => Some(columns),
    Err(err) => {
      debug!("Failed to get column def: {err}");
      None
    }
  };

  let mut json_rows: Vec<Vec<serde_json::Value>> = vec![];
  for (idx, row) in rows.iter().enumerate() {
    if idx >= limit {
      break;
    }

    json_rows.push(row_to_json_array(row)?);
  }

  return Ok((json_rows, columns));
}

#[cfg(test)]
mod tests {

  use serde_json::json;

  use super::*;
  use crate::app_state::*;
  use crate::constants::USER_TABLE;
  use crate::table_metadata::{lookup_and_parse_table_schema, TableMetadata};

  #[tokio::test]
  async fn test_read_rows() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    let pattern = serde_json::from_str(
      r#"{
          "type": "object",
          "additionalProperties": false,
          "properties": {
            "name": {
              "type": "string"
            },
            "obj": {
              "type": "object"
            }
          },
          "required": ["name", "obj"]
        }"#,
    )
    .unwrap();

    trailbase_schema::registry::set_user_schema("foo", Some(pattern)).unwrap();
    conn
      .execute(
        format!(
          r#"CREATE TABLE test_table (
            col0 TEXT CHECK(jsonschema('foo', col0))
          ) STRICT"#
        ),
        (),
      )
      .await
      .unwrap();

    let table = lookup_and_parse_table_schema(conn, "test_table")
      .await
      .unwrap();
    let metadata = TableMetadata::new(table.clone(), &[table], USER_TABLE);

    let insert = |json: serde_json::Value| async move {
      conn
        .execute(
          format!(
            "INSERT INTO test_table (col0) VALUES ('{}')",
            json.to_string()
          ),
          (),
        )
        .await
    };

    let object = json!({"name": "foo", "obj": json!({
      "a": "b",
      "c": 42,
    })});
    insert(object.clone()).await.unwrap();

    let rows = conn
      .read_query_rows("SELECT * FROM test_table", ())
      .await
      .unwrap();
    let parsed = rows_to_json(
      &metadata.schema.columns,
      &metadata.json_metadata.columns,
      rows,
      |_| true,
    )
    .unwrap();

    assert_eq!(parsed.len(), 1);
    let serde_json::Value::Object(map) = parsed.first().unwrap() else {
      panic!("expected object");
    };
    assert_eq!(map.get("col0").unwrap().clone(), object);
  }
}
