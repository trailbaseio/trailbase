use itertools::Itertools;
use log::*;
use rusqlite::types;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use trailbase_schema::QualifiedName;
use trailbase_schema::json::value_to_flat_json;
use trailbase_schema::sqlite::{Column, ColumnOption};

use crate::records::RecordError;
use crate::schema_metadata::{JsonColumnMetadata, SchemaMetadataCache, TableMetadata};

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
  UnexpectedType(&'static str, trailbase_schema::sqlite::ColumnDataType),
  #[error("Parse int error: {0}")]
  ParseInt(#[from] std::num::ParseIntError),
  #[error("Parse float error: {0}")]
  ParseFloat(#[from] std::num::ParseFloatError),
  // NOTE: This is the only extra error to schema::JsonError. Can we collapse?
  #[error("SerdeJson error: {0}")]
  SerdeJson(#[from] serde_json::Error),
}

impl From<trailbase_schema::json::JsonError> for JsonError {
  fn from(value: trailbase_schema::json::JsonError) -> Self {
    return match value {
      trailbase_schema::json::JsonError::Finite => Self::Finite,
      trailbase_schema::json::JsonError::ValueNotFound => Self::ValueNotFound,
      trailbase_schema::json::JsonError::NotSupported => Self::NotSupported,
      trailbase_schema::json::JsonError::Decode(err) => Self::Decode(err),
      trailbase_schema::json::JsonError::UnexpectedType(expected, got) => {
        Self::UnexpectedType(expected, got)
      }
      trailbase_schema::json::JsonError::ParseInt(err) => Self::ParseInt(err),
      trailbase_schema::json::JsonError::ParseFloat(err) => Self::ParseFloat(err),
    };
  }
}

#[inline]
fn is_foreign_key(options: &[ColumnOption]) -> bool {
  return options
    .iter()
    .any(|o| matches!(o, ColumnOption::ForeignKey { .. }));
}

/// Serialize SQL row to json.
pub(crate) fn row_to_json_expand(
  columns: &[Column],
  json_metadata: &[Option<JsonColumnMetadata>],
  row: &trailbase_sqlite::Row,
  column_filter: fn(&str) -> bool,
  expand: Option<&HashMap<String, serde_json::Value>>,
) -> Result<serde_json::Value, JsonError> {
  // Row may contain extra columns like trailing "_rowid_".
  assert!(columns.len() <= row.column_count());
  assert_eq!(columns.len(), json_metadata.len());

  return Ok(serde_json::Value::Object(
    (0..columns.len())
      .filter(|i| column_filter(&columns[*i].name))
      .map(|i| -> Result<(String, serde_json::Value), JsonError> {
        let column = &columns[i];

        assert_eq!(Some(column.name.as_str()), row.column_name(i));

        let value = row.get_value(i).ok_or(JsonError::ValueNotFound)?;
        if matches!(value, types::Value::Null) {
          return Ok((column.name.clone(), serde_json::Value::Null));
        }

        if let Some(foreign_value) = expand.and_then(|e| e.get(&column.name)) {
          if is_foreign_key(&column.options) {
            let id = value_to_flat_json(value)?;

            return Ok(match foreign_value {
              serde_json::Value::Null => (
                column.name.clone(),
                serde_json::json!({
                  "id": id,
                }),
              ),
              value => (
                column.name.clone(),
                serde_json::json!({
                  "id": id,
                  "data": value,
                }),
              ),
            });
          }
        }

        if let types::Value::Text(str) = &value {
          if json_metadata[i].is_some() {
            return Ok((column.name.clone(), serde_json::from_str(str)?));
          }
        }

        return Ok((column.name.clone(), value_to_flat_json(value)?));
      })
      .collect::<Result<_, JsonError>>()?,
  ));
}

pub(crate) struct ExpandedTable {
  pub metadata: Arc<TableMetadata>,
  pub local_column_name: String,
  pub num_columns: usize,

  pub foreign_table_name: String,
  pub foreign_column_name: String,
}

pub(crate) fn expand_tables<'a, 'b, T: AsRef<str>>(
  schema_metadata: &SchemaMetadataCache,
  database_schema: &Option<String>,
  root_column_by_name: impl Fn(&'a str) -> Option<&'b Column>,
  expand: &'a [T],
) -> Result<Vec<ExpandedTable>, RecordError> {
  let mut expanded_tables = Vec::<ExpandedTable>::with_capacity(expand.len());

  for col_name in expand {
    let col_name = col_name.as_ref();
    if col_name.is_empty() {
      continue;
    }
    let Some(column) = root_column_by_name(col_name) else {
      return Err(RecordError::Internal("Missing column".into()));
    };

    // FIXME: This only expand FKs expressed as column constraints missing table constraints.
    let Some(ColumnOption::ForeignKey {
      foreign_table: foreign_table_name,
      referred_columns: _,
      ..
    }) = column
      .options
      .iter()
      .find_or_first(|o| matches!(o, ColumnOption::ForeignKey { .. }))
    else {
      return Err(RecordError::Internal("not a foreign key".into()));
    };

    let Some(foreign_table) = schema_metadata.get_table(&QualifiedName {
      name: foreign_table_name.clone(),
      database_schema: database_schema.clone(),
    }) else {
      return Err(RecordError::ApiRequiresTable);
    };

    let Some(foreign_pk_column_idx) = foreign_table.record_pk_column else {
      return Err(RecordError::Internal("invalid PK".into()));
    };

    let foreign_pk_column = &foreign_table.schema.columns[foreign_pk_column_idx].name;

    // TODO: Check that `referred_columns` and foreign_pk_column are the same. It's already
    // validated as part of config validation.

    let num_columns = foreign_table.schema.columns.len();
    let foreign_table_name = foreign_table_name.to_string();
    let foreign_column_name = foreign_pk_column.to_string();

    expanded_tables.push(ExpandedTable {
      metadata: foreign_table,
      local_column_name: col_name.to_string(),
      num_columns,
      foreign_table_name,
      foreign_column_name,
    });
  }

  return Ok(expanded_tables);
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

    let table = lookup_and_parse_table_schema(conn, "test_table", Some("main"))
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

    let parsed = rows
      .iter()
      .map(|row| {
        row_to_json_expand(
          &metadata.schema.columns,
          &metadata.json_metadata.columns,
          row,
          |_| true,
          None,
        )
      })
      .collect::<Result<Vec<_>, _>>()
      .unwrap();

    assert_eq!(parsed.len(), 1);
    let serde_json::Value::Object(map) = parsed.first().unwrap() else {
      panic!("expected object");
    };
    assert_eq!(map.get("col0").unwrap().clone(), object);
  }
}
