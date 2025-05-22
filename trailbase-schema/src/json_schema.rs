use jsonschema::Validator;
use log::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::metadata::{JsonColumnMetadata, JsonSchemaError, TableMetadata, extract_json_metadata};
use crate::sqlite::{Column, ColumnDataType, ColumnOption};

/// Influeces the generated JSON schema. In `Insert` mode columns with default values will be
/// optional.
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum JsonSchemaMode {
  /// Insert mode.
  Insert,
  /// Read/Select mode.
  Select,
  /// Update mode.
  Update,
}

/// Builds a JSON Schema definition for the given table.
///
/// NOTE: insert and select require different types to model default values, i.e. a column with a
/// default value is optional during insert but guaranteed during reads.
///
/// NOTE: We're not currently respecting the RecordApi `autofill_missing_user_id_columns`
/// setting. Not sure we should since this is more a feature for no-JS, HTTP-only apps, which
/// don't benefit from type-safety anyway.
pub fn build_json_schema(
  title: &str,
  columns: &[Column],
  mode: JsonSchemaMode,
) -> Result<(Validator, serde_json::Value), JsonSchemaError> {
  return build_json_schema_expanded(title, columns, mode, None);
}

#[derive(Debug)]
pub struct Expand<'a> {
  pub tables: &'a [TableMetadata],
  pub foreign_key_columns: Vec<&'a str>,
}

/// NOTE: Foreign keys can only reference tables not view, so the inline schemas don't need to be
/// able to reference views.
pub fn build_json_schema_expanded(
  title: &str,
  columns: &[Column],
  mode: JsonSchemaMode,
  expand: Option<Expand<'_>>,
) -> Result<(Validator, serde_json::Value), JsonSchemaError> {
  let mut properties = serde_json::Map::new();
  let mut defs = serde_json::Map::new();
  let mut required_cols: Vec<String> = vec![];

  for col in columns {
    let mut def_name: Option<String> = None;
    let mut not_null = false;
    let mut default = false;

    for opt in &col.options {
      match opt {
        ColumnOption::NotNull => not_null = true,
        ColumnOption::Default(_) => default = true,
        ColumnOption::Check(check) => {
          if let Some(json_metadata) = extract_json_metadata(&ColumnOption::Check(check.clone()))? {
            let new_def_name = &col.name;
            match json_metadata {
              JsonColumnMetadata::SchemaName(name) => {
                let Some(schema) = crate::registry::get_schema(&name) else {
                  return Err(JsonSchemaError::NotFound(name.to_string()));
                };
                defs.insert(new_def_name.clone(), schema.schema);
                def_name = Some(new_def_name.clone());
              }
              JsonColumnMetadata::Pattern(pattern) => {
                defs.insert(new_def_name.clone(), pattern.clone());
                def_name = Some(new_def_name.clone());
              }
            }
          }
        }
        ColumnOption::Unique { is_primary, .. } => {
          // According to the SQL standard, PRIMARY KEY should always imply NOT NULL.
          // Unfortunately, due to a bug in some early versions, this is not the case in SQLite.
          // Unless the column is an INTEGER PRIMARY KEY or the table is a WITHOUT ROWID table or a
          // STRICT table or the column is declared NOT NULL, SQLite allows NULL values in a
          // PRIMARY KEY column
          // source: https://www.sqlite.org/lang_createtable.html
          if *is_primary {
            if col.data_type == ColumnDataType::Integer {
              not_null = true;
            }

            default = true;
          }
        }
        ColumnOption::ForeignKey {
          foreign_table,
          referred_columns,
          ..
        } => {
          if let (Some(expand), JsonSchemaMode::Select) = (&expand, mode) {
            let column_is_expanded = expand
              .foreign_key_columns
              .iter()
              .any(|column_name| *column_name != col.name);
            if !column_is_expanded {
              continue;
            }

            // NOTE: Foreign keys cannot cross database boundaries, we can therefore compare by
            // unqualified name.
            let Some(table) = expand
              .tables
              .iter()
              .find(|t| t.name().name == *foreign_table)
            else {
              warn!("Failed to find table: {foreign_table}");
              continue;
            };

            let Some(pk_column) = (match referred_columns.len() {
              0 => crate::metadata::find_pk_column_index(&table.schema.columns)
                .map(|idx| &table.schema.columns[idx]),
              1 => table
                .schema
                .columns
                .iter()
                .find(|c| c.name == referred_columns[0]),
              _ => {
                warn!("Skipping. Expected single referred column : {referred_columns:?}");
                continue;
              }
            }) else {
              warn!("Failed to find pk column for {:?}", table.name());
              continue;
            };

            let (_validator, schema) =
              build_json_schema(foreign_table, &table.schema.columns, mode)?;

            let new_def_name = foreign_table.clone();
            defs.insert(
              new_def_name.clone(),
              serde_json::json!({
                "type": "object",
                "properties": {
                  "id": {
                    "type": column_data_type_to_json_type(pk_column.data_type),
                  },
                  "data": schema,
                },
                "required": ["id"],
              }),
            );
            def_name = Some(new_def_name);
          }
        }
        _ => {}
      }
    }

    match mode {
      JsonSchemaMode::Insert => {
        if not_null && !default {
          required_cols.push(col.name.clone());
        }
      }
      JsonSchemaMode::Select => {
        if not_null {
          required_cols.push(col.name.clone());
        }
      }
      JsonSchemaMode::Update => {}
    }

    properties.insert(
      col.name.clone(),
      if let Some(def_name) = def_name {
        serde_json::json!({
          "$ref": format!("#/$defs/{def_name}")
        })
      } else {
        serde_json::json!({
          "type": column_data_type_to_json_type(col.data_type),
        })
      },
    );
  }

  let schema = if defs.is_empty() {
    serde_json::json!({
      "title": title,
      "type": "object",
      "properties": serde_json::Value::Object(properties),
      "required": serde_json::json!(required_cols),
    })
  } else {
    serde_json::json!({
      "title": title,
      "type": "object",
      "properties": serde_json::Value::Object(properties),
      "required": serde_json::json!(required_cols),
      "$defs": serde_json::Value::Object(defs),
    })
  };

  return Ok((
    Validator::new(&schema).map_err(|err| JsonSchemaError::SchemaCompile(err.to_string()))?,
    schema,
  ));
}

fn column_data_type_to_json_type(data_type: ColumnDataType) -> Value {
  return match data_type {
    ColumnDataType::Null => Value::String("null".into()),
    ColumnDataType::Any => Value::Array(vec![
      "number".into(),
      "string".into(),
      "boolean".into(),
      "object".into(),
      "array".into(),
      "null".into(),
    ]),
    ColumnDataType::Text => Value::String("string".into()),
    // We encode all blobs as url-safe Base64.
    ColumnDataType::Blob => Value::String("string".into()),
    ColumnDataType::Integer => Value::String("integer".into()),
    ColumnDataType::Real => Value::String("number".into()),
    ColumnDataType::Numeric => Value::String("number".into()),
    // JSON types
    ColumnDataType::JSON => Value::String("object".into()),
    ColumnDataType::JSONB => Value::String("object".into()),
    // Affine types
    //
    // Integers:
    ColumnDataType::Int => Value::String("number".into()),
    ColumnDataType::TinyInt => Value::String("number".into()),
    ColumnDataType::SmallInt => Value::String("number".into()),
    ColumnDataType::MediumInt => Value::String("number".into()),
    ColumnDataType::BigInt => Value::String("number".into()),
    ColumnDataType::UnignedBigInt => Value::String("number".into()),
    ColumnDataType::Int2 => Value::String("number".into()),
    ColumnDataType::Int4 => Value::String("number".into()),
    ColumnDataType::Int8 => Value::String("number".into()),
    // Text:
    ColumnDataType::Character => Value::String("string".into()),
    ColumnDataType::Varchar => Value::String("string".into()),
    ColumnDataType::VaryingCharacter => Value::String("string".into()),
    ColumnDataType::NChar => Value::String("string".into()),
    ColumnDataType::NativeCharacter => Value::String("string".into()),
    ColumnDataType::NVarChar => Value::String("string".into()),
    ColumnDataType::Clob => Value::String("string".into()),
    // Real:
    ColumnDataType::Double => Value::String("number".into()),
    ColumnDataType::DoublePrecision => Value::String("number".into()),
    ColumnDataType::Float => Value::String("number".into()),
    // Numeric:
    ColumnDataType::Boolean => Value::String("boolean".into()),
    ColumnDataType::Decimal => Value::String("number".into()),
    ColumnDataType::Date => Value::String("number".into()),
    ColumnDataType::DateTime => Value::String("number".into()),
  };
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use crate::FileUpload;
  use crate::sqlite::{ColumnOption, lookup_and_parse_table_schema};

  use super::*;

  #[tokio::test]
  async fn test_parse_table_schema() {
    crate::registry::try_init_schemas();

    let conn = trailbase_extension::connect_sqlite(None, None).unwrap();

    let check = indoc::indoc! {r#"
        jsonschema_matches ('{
          "type": "object",
          "additionalProperties": false,
          "properties": {
            "name": {
              "type": "string"
            },
            "age": {
              "type": "integer",
              "minimum": 0
            }
          },
          "required": ["name", "age"]
        }', col0)"#
    };

    conn
      .execute(
        &format!(
          r#"CREATE TABLE test_table (
            col0 TEXT CHECK({check}),
            col1 TEXT CHECK(jsonschema('std.FileUpload', col1)),
            col2 TEXT,
            col3 TEXT CHECK(jsonschema('std.FileUpload', col3, 'image/jpeg, image/png'))
          ) STRICT"#
        ),
        (),
      )
      .unwrap();

    let insert = |col: &'static str, json: serde_json::Value| {
      conn.execute(
        &format!(
          "INSERT INTO test_table ({col}) VALUES ('{}')",
          json.to_string()
        ),
        (),
      )
    };

    assert!(insert("col2", json!({"name": 42})).unwrap() > 0);
    assert!(
      insert(
        "col1",
        serde_json::to_value(FileUpload::new(
          uuid::Uuid::now_v7(),
          Some("filename".to_string()),
          None,
          None
        ))
        .unwrap()
      )
      .unwrap()
        > 0
    );
    assert!(insert("col1", json!({"foo": "/foo"})).is_err());
    assert!(insert("col0", json!({"name": 42})).is_err());
    assert!(insert("col0", json!({"name": "Alice"})).is_err());
    assert!(insert("col0", json!({"name": "Alice", "age": 23})).unwrap() > 0);
    assert!(
      insert(
        "col0",
        json!({"name": "Alice", "age": 23, "additional": 42})
      )
      .is_err()
    );

    assert!(insert("col3", json!({"foo": "/foo"})).is_err());
    assert!(
      insert(
        "col3",
        json!({
            "id": uuid::Uuid::now_v7().to_string(),
            // Missing mime-type.
        })
      )
      .is_err()
    );
    assert!(insert("col3", json!({"mime_type": "invalid"})).is_err());
    assert!(
      insert(
        "col3",
        json!({
          "id": uuid::Uuid::now_v7().to_string(),
          "mime_type": "image/png"
        })
      )
      .is_ok()
    );

    let cnt: i64 = conn
      .query_row("SELECT COUNT(*) FROM test_table", (), |row| row.get(0))
      .unwrap();

    assert_eq!(cnt, 4);

    let table = lookup_and_parse_table_schema(&conn, "test_table").unwrap();

    let col = table.columns.first().unwrap();
    let check_expr = col
      .options
      .iter()
      .filter_map(|c| match c {
        ColumnOption::Check(check) => Some(check),
        _ => None,
      })
      .collect::<Vec<_>>()[0];

    assert_eq!(check_expr, check);
    let table_metadata = TableMetadata::new(table.clone(), &[table], "_user");

    let (schema, _) = build_json_schema(
      &table_metadata.name().name,
      &table_metadata.schema.columns,
      JsonSchemaMode::Insert,
    )
    .unwrap();
    assert!(schema.is_valid(&json!({
      "col2": "test",
    })));

    assert!(schema.is_valid(&json!({
      "col0": json!({
        "name": "Alice", "age": 23,
      }),
    })));

    assert!(!schema.is_valid(&json!({
      "col0": json!({
        "name": 42, "age": "23",
      }),
    })));
  }
}
