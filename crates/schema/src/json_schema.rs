use jsonschema::Validator;
use log::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::LazyLock;
use trailbase_extension::jsonschema::JsonSchemaRegistry;

use crate::metadata::{
  ColumnMetadata, JsonColumnMetadata, JsonSchemaError, TableMetadata, extract_json_metadata,
  is_pk_column,
};
use crate::sqlite::{ColumnDataType, ColumnOption};

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
  registry: &JsonSchemaRegistry,
  title: &str,
  columns: &[ColumnMetadata],
  mode: JsonSchemaMode,
) -> Result<(Validator, serde_json::Value), JsonSchemaError> {
  return build_json_schema_expanded(registry, title, columns, mode, None);
}

#[derive(Debug)]
pub struct Expand<'a> {
  pub tables: &'a [&'a TableMetadata],
  pub foreign_key_columns: Vec<&'a str>,
}

/// NOTE: Foreign keys can only reference tables not view, so the inline schemas don't need to be
/// able to reference views.
pub fn build_json_schema_expanded(
  registry: &JsonSchemaRegistry,
  title: &str,
  columns_metadata: &[ColumnMetadata],
  mode: JsonSchemaMode,
  expand: Option<Expand<'_>>,
) -> Result<(Validator, serde_json::Value), JsonSchemaError> {
  let mut schema =
    build_json_schema_expanded_impl(registry, title, columns_metadata, mode, expand)?;

  if let Some(obj) = schema.as_object_mut() {
    const SCHEMA_STD: &str = "https://json-schema.org/draft/2020-12/schema";
    obj.insert("$schema".to_string(), SCHEMA_STD.into());
  }

  return Ok((
    Validator::new(&schema).map_err(|err| JsonSchemaError::SchemaCompile(err.to_string()))?,
    schema,
  ));
}

fn build_json_schema_expanded_impl(
  registry: &JsonSchemaRegistry,
  title: &str,
  columns_metadata: &[ColumnMetadata],
  mode: JsonSchemaMode,
  expand: Option<Expand<'_>>,
) -> Result<serde_json::Value, JsonSchemaError> {
  let mut properties = serde_json::Map::new();
  let mut defs = serde_json::Map::new();
  let mut required_cols: Vec<String> = vec![];

  for meta in columns_metadata {
    let col = &meta.column;

    let mut def_name: Option<String> = None;
    let mut not_null = false;
    let mut default = false;

    for opt in &col.options {
      match opt {
        ColumnOption::NotNull => not_null = true,
        ColumnOption::Default(_) => default = true,
        ColumnOption::Check(_) => {
          let Some(json_metadata) = extract_json_metadata(registry, opt)? else {
            continue;
          };
          debug_assert_eq!(Some(&json_metadata), meta.json.as_ref());

          match json_metadata {
            JsonColumnMetadata::SchemaName(name) => {
              let Some(entry) = registry.get_schema(&name) else {
                return Err(JsonSchemaError::NotFound(name.to_string()));
              };

              let Some(schema_obj) = entry.schema.as_object() else {
                return Err(JsonSchemaError::Other("expected object".to_string()));
              };

              // Re-parent nested references to the schema root, to continue to be reference-able
              // via: `{"$ref": "#/$defs/<name>"}`, otherwise they won't be found.
              //
              // QUESTION: is there a better API for us to merge JSON schemas, w/o that manual
              // work
              if let Some(nested_defs) = schema_obj.get("$defs").and_then(|d| d.as_object()) {
                for (k, v) in nested_defs {
                  defs.insert(k.clone(), v.clone());
                }
              }

              defs.insert(col.name.clone(), entry.schema.clone());
              def_name = Some(col.name.clone());
            }
            JsonColumnMetadata::Pattern(pattern) => {
              defs.insert(col.name.clone(), pattern.clone());
              def_name = Some(col.name.clone());
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
              0 => table.schema.columns.iter().find(|c| is_pk_column(c)),
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

            let nested_schema = build_json_schema_expanded_impl(
              registry,
              foreign_table,
              &table.column_metadata,
              mode,
              None,
            )?;

            let new_def_name = foreign_table.clone();
            defs.insert(
              new_def_name.clone(),
              serde_json::json!({
                "type": "object",
                "properties": {
                  "id": {
                    "type": column_data_type_to_json_type(pk_column.data_type),
                  },
                  "data": nested_schema,
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

    if meta.is_geometry {
      const KEY: &str = "_geojson_geometry";
      defs.insert(KEY.to_string(), GEOJSON_GEOMETRY.clone());
      def_name = Some(KEY.to_string());
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

  if defs.is_empty() {
    return Ok(serde_json::json!({
      "title": title,
      "type": "object",
      "properties": serde_json::Value::Object(properties),
      "required": serde_json::json!(required_cols),
    }));
  }

  return Ok(serde_json::json!({
    "title": title,
    "type": "object",
    "properties": serde_json::Value::Object(properties),
    "required": serde_json::json!(required_cols),
    "$defs": serde_json::Value::Object(defs),
  }));
}

fn column_data_type_to_json_type(data_type: ColumnDataType) -> Value {
  return match data_type {
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
  };
}

static GEOJSON_GEOMETRY: LazyLock<Value> = LazyLock::new(|| {
  const GEOJSON_GEOMETRY: &[u8] = include_bytes!("../schemas/Geometry.json");
  return serde_json::from_slice(GEOJSON_GEOMETRY).expect("valid");
});

#[cfg(test)]
mod tests {
  use parking_lot::RwLock;
  use serde_json::json;
  use std::sync::Arc;

  use crate::FileUpload;
  use crate::sqlite::{ColumnOption, Table, lookup_and_parse_table_schema};

  use super::*;

  #[test]
  fn test_parse_table_schema() {
    let registry = Arc::new(RwLock::new(
      crate::registry::build_json_schema_registry(vec![]).unwrap(),
    ));
    let conn = trailbase_extension::connect_sqlite(None, Some(registry.clone())).unwrap();

    let col0_schema = json!({
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
    });

    conn
      .execute(
        &format!(
          r#"CREATE TABLE test_table (
            col0 TEXT CHECK(jsonschema_matches('{col0_schema}', col0)),
            col1 TEXT CHECK(jsonschema('std.FileUpload', col1)),
            col2 TEXT,
            col3 TEXT CHECK(jsonschema('std.FileUpload', col3, 'image/jpeg, image/png'))
          ) STRICT"#
        ),
        (),
      )
      .unwrap();

    let (table, schema, _value) = get_and_build_table_schema(
      &conn,
      &registry.read(),
      "test_table",
      JsonSchemaMode::Insert,
    );

    let col = table.columns.first().unwrap();
    let check_expr = col
      .options
      .iter()
      .filter_map(|c| match c {
        ColumnOption::Check(check) => Some(check),
        _ => None,
      })
      .collect::<Vec<_>>()[0];

    assert_eq!(
      &format!("jsonschema_matches ('{col0_schema}', col0)"),
      check_expr
    );

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

    // Make sure, schemas are applied correctly by inserting records with appropriate and
    // inappropriate shapes.
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
          "id": uuid::Uuid::new_v4().to_string(),
          "filename": "foo_o3uoiuo.png",
          "mime_type": "image/png"
        })
      )
      .is_ok()
    );

    let cnt: i64 = conn
      .query_row("SELECT COUNT(*) FROM test_table", (), |row| row.get(0))
      .unwrap();

    assert_eq!(cnt, 4);
  }

  #[test]
  fn test_file_uploads_schema() {
    let registry = Arc::new(RwLock::new(
      crate::registry::build_json_schema_registry(vec![]).unwrap(),
    ));
    let conn = trailbase_extension::connect_sqlite(None, Some(registry.clone())).unwrap();

    conn
      .execute(
        &format!(
          r#"CREATE TABLE test_table (
            files TEXT CHECK(jsonschema('std.FileUploads', files))
          ) STRICT"#
        ),
        (),
      )
      .unwrap();

    let (_table, schema, _value) = get_and_build_table_schema(
      &conn,
      &registry.read(),
      "test_table",
      JsonSchemaMode::Insert,
    );

    assert!(schema.is_valid(&json!({})));
  }

  #[test]
  fn test_geojson_schema() {
    use rusqlite::functions::FunctionFlags;

    let registry = Arc::new(RwLock::new(
      crate::registry::build_json_schema_registry(vec![]).unwrap(),
    ));
    let conn = trailbase_extension::connect_sqlite(None, Some(registry.clone())).unwrap();
    conn
      .create_scalar_function(
        "ST_IsValid",
        1,
        FunctionFlags::SQLITE_INNOCUOUS
          | FunctionFlags::SQLITE_UTF8
          | FunctionFlags::SQLITE_DETERMINISTIC,
        |_ctx| return Ok(true),
      )
      .unwrap();

    conn
      .execute_batch("CREATE TABLE test_table (geom BLOB NOT NULL CHECK(ST_IsValid(geom))) STRICT;")
      .unwrap();

    {
      // Insert
      let (_table, schema, _value) = get_and_build_table_schema(
        &conn,
        &registry.read(),
        "test_table",
        JsonSchemaMode::Insert,
      );

      let valid_point = json!({
        "type": "Point",
        "coordinates": [125.6, 10.1]
      });
      assert!(schema.is_valid(&json!({
        "geom": valid_point,
      })));

      assert!(!schema.is_valid(&json!({})));

      let invalid_point = json!({
        "type": "Point",
        "coordinates": [125.6]
      });
      assert!(
        !schema.is_valid(&json!({
        "geom": invalid_point,
          })),
        "{schema:?},\n{}",
        serde_json::to_string_pretty(&_value).unwrap()
      );
    }

    {
      // Update
      let (_table, schema, _value) = get_and_build_table_schema(
        &conn,
        &registry.read(),
        "test_table",
        JsonSchemaMode::Update,
      );

      assert!(schema.is_valid(&json!({})));
    }
  }

  fn get_and_build_table_schema(
    conn: &rusqlite::Connection,
    registry: &JsonSchemaRegistry,
    table_name: &str,
    mode: JsonSchemaMode,
  ) -> (Table, Validator, Value) {
    let table = lookup_and_parse_table_schema(conn, table_name).unwrap();

    let table_metadata = TableMetadata::new(&registry, table.clone(), &[table.clone()]).unwrap();
    let (schema, value) = build_json_schema(
      &registry,
      &table_metadata.name().name,
      &table_metadata.column_metadata,
      mode,
    )
    .unwrap();

    return (table, schema, value);
  }
}
