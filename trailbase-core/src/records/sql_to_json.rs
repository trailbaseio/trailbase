use base64::prelude::*;
use log::*;
use thiserror::Error;

use crate::schema::{Column, ColumnDataType};
use crate::table_metadata::TableOrViewMetadata;

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
}

fn value_to_json(value: libsql::Value) -> Result<serde_json::Value, JsonError> {
  return Ok(match value {
    libsql::Value::Null => serde_json::Value::Null,
    libsql::Value::Real(real) => {
      let Some(number) = serde_json::Number::from_f64(real) else {
        return Err(JsonError::Finite);
      };
      serde_json::Value::Number(number)
    }
    libsql::Value::Integer(integer) => serde_json::Value::Number(serde_json::Number::from(integer)),
    libsql::Value::Blob(blob) => serde_json::Value::String(BASE64_URL_SAFE.encode(blob)),
    libsql::Value::Text(text) => serde_json::Value::String(text),
  });
}

// Serialize libsql row to json.
pub fn row_to_json(
  metadata: &(dyn TableOrViewMetadata + Send + Sync),
  row: libsql::Row,
  column_filter: fn(&str) -> bool,
) -> Result<serde_json::Value, JsonError> {
  let mut map = serde_json::Map::<String, serde_json::Value>::default();

  for i in 0..(row.column_count()) {
    let Some(col_name) = row.column_name(i) else {
      error!("Missing column name for {i} in  {row:?}");
      continue;
    };
    if !column_filter(col_name) {
      continue;
    }

    let value = row.get_value(i).map_err(|_err| JsonError::ValueNotFound)?;
    if let libsql::Value::Text(str) = &value {
      if let Some((_col, col_meta)) = metadata.column_by_name(col_name) {
        if col_meta.json.is_some() {
          map.insert(col_name.to_string(), serde_json::from_str(str)?);
          continue;
        }
      } else {
        warn!("Missing col: {col_name}");
      }
    }

    map.insert(col_name.to_string(), value_to_json(value)?);
  }

  return Ok(serde_json::Value::Object(map));
}

// Turns rows into a list of json objects.
pub async fn rows_to_json(
  metadata: &(dyn TableOrViewMetadata + Send + Sync),
  mut rows: libsql::Rows,
  column_filter: fn(&str) -> bool,
) -> Result<Vec<serde_json::Value>, JsonError> {
  let mut objects: Vec<serde_json::Value> = vec![];

  while let Some(row) = rows.next().await.map_err(|_err| JsonError::RowNotFound)? {
    objects.push(row_to_json(metadata, row, column_filter)?);
  }

  return Ok(objects);
}

/// Turns a row into a list of json arrays.
pub fn row_to_json_array(row: libsql::Row) -> Result<Vec<serde_json::Value>, JsonError> {
  let cols = row.column_count();
  let mut json_row = Vec::<serde_json::Value>::with_capacity(cols as usize);

  for i in 0..cols {
    let value = row.get_value(i).map_err(|_err| JsonError::ValueNotFound)?;
    json_row.push(value_to_json(value)?);
  }

  return Ok(json_row);
}

/// Best-effort conversion from row values to column definition.
///
/// WARN: This is lossy and whenever possible we should rely on parsed "CREATE TABLE" statement for
/// the respective column.
fn rows_to_columns(rows: &libsql::Rows) -> Result<Vec<Column>, libsql::Error> {
  use libsql::ValueType as T;

  let mut columns: Vec<Column> = vec![];
  for i in 0..rows.column_count() {
    columns.push(Column {
      name: rows.column_name(i).unwrap_or("<missing>").to_string(),
      data_type: match rows.column_type(i)? {
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

/// Turns rows into a list of json arrays.
pub async fn rows_to_json_arrays(
  mut rows: libsql::Rows,
  limit: usize,
) -> Result<(Vec<Vec<serde_json::Value>>, Option<Vec<Column>>), JsonError> {
  let mut cnt = 0_usize;

  let columns = rows_to_columns(&rows).ok();

  let mut json_rows: Vec<Vec<serde_json::Value>> = vec![];
  while let Some(row) = rows.next().await.map_err(|_err| JsonError::RowNotFound)? {
    if cnt >= limit {
      break;
    }
    cnt += 1;

    json_rows.push(row_to_json_array(row)?);
  }

  return Ok((json_rows, columns));
}

#[cfg(test)]
mod tests {

  use serde_json::json;

  use super::*;
  use crate::app_state::*;
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

    trailbase_sqlite::schema::set_user_schema("foo", Some(pattern)).unwrap();
    conn
      .execute(
        &format!(
          r#"CREATE TABLE test_table (
            col0 TEXT CHECK(jsonschema('foo', col0))
          ) strict"#
        ),
        (),
      )
      .await
      .unwrap();

    let table = lookup_and_parse_table_schema(conn, "test_table")
      .await
      .unwrap();
    let metadata = TableMetadata::new(table.clone(), &[table]);

    let insert = |json: serde_json::Value| async move {
      conn
        .execute(
          &format!(
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

    let rows = conn.query("SELECT * FROM test_table", ()).await.unwrap();
    let parsed = rows_to_json(&metadata, rows, |_| true).await.unwrap();

    assert_eq!(parsed.len(), 1);
    let serde_json::Value::Object(map) = parsed.first().unwrap() else {
      panic!("expected object");
    };
    assert_eq!(map.get("col0").unwrap().clone(), object);
  }
}
