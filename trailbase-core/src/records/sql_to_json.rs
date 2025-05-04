use log::*;
use std::collections::HashMap;
use thiserror::Error;
use trailbase_schema::sqlite::Column;
use trailbase_schema::sqlite::ColumnOption;
use trailbase_sqlite::rows::value_to_json;

use crate::schema_metadata::JsonColumnMetadata;

#[derive(Debug, Error)]
pub enum JsonError {
  #[error("SerdeJson error: {0}")]
  SerdeJson(#[from] serde_json::Error),
  #[error("Float not finite")]
  Finite,
  #[error("Value not found")]
  ValueNotFound,
  #[error("Missing col name")]
  MissingColumnName,
}

impl From<trailbase_sqlite::rows::JsonError> for JsonError {
  fn from(value: trailbase_sqlite::rows::JsonError) -> Self {
    return match value {
      trailbase_sqlite::rows::JsonError::ValueNotFound => Self::ValueNotFound,
      trailbase_sqlite::rows::JsonError::Finite => Self::Finite,
    };
  }
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
          let id = match value_to_json(value) {
            Ok(value) => value,
            Err(err) => {
              return Some(Err(err.into()));
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

      return match value_to_json(value) {
        Ok(value) => Some(Ok((column_name.to_string(), value))),
        Err(err) => Some(Err(err.into())),
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

#[cfg(test)]
mod tests {

  use serde_json::json;

  use super::*;
  use crate::app_state::*;
  use crate::constants::USER_TABLE;
  use crate::schema_metadata::{TableMetadata, lookup_and_parse_table_schema};

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
