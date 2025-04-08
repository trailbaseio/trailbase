use fallible_iterator::FallibleIterator;
use log::*;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use trailbase_schema::sqlite::{sqlite3_parse_into_statement, SchemaError, Table, View};
use trailbase_sqlite::params;

pub use trailbase_schema::metadata::{
  JsonColumnMetadata, JsonSchemaError, TableMetadata, TableOrViewMetadata, ViewMetadata,
};

use crate::constants::{SQLITE_SCHEMA_TABLE, USER_TABLE};

struct TableMetadataCacheState {
  tables: HashMap<String, Arc<TableMetadata>>,
  views: HashMap<String, Arc<ViewMetadata>>,
}

#[derive(Clone)]
pub struct TableMetadataCache {
  conn: trailbase_sqlite::Connection,
  state: Arc<parking_lot::RwLock<TableMetadataCacheState>>,
}

impl TableMetadataCache {
  pub async fn new(conn: trailbase_sqlite::Connection) -> Result<Self, TableLookupError> {
    let tables = lookup_and_parse_all_table_schemas(&conn).await?;
    let table_map = Self::build_tables(&conn, &tables).await?;
    let views = Self::build_views(&conn, &tables).await?;

    return Ok(TableMetadataCache {
      conn,
      state: Arc::new(parking_lot::RwLock::new(TableMetadataCacheState {
        tables: table_map,
        views,
      })),
    });
  }

  async fn build_tables(
    conn: &trailbase_sqlite::Connection,
    tables: &[Table],
  ) -> Result<HashMap<String, Arc<TableMetadata>>, TableLookupError> {
    let table_metadata_map: HashMap<String, Arc<TableMetadata>> = tables
      .iter()
      .cloned()
      .map(|t: Table| {
        (
          t.name.clone(),
          Arc::new(TableMetadata::new(t, tables, USER_TABLE)),
        )
      })
      .collect();

    // Install file column triggers. This ain't pretty, this might be better on construction and
    // schema changes.
    for metadata in table_metadata_map.values() {
      for idx in metadata.json_metadata.file_column_indexes() {
        let table_name = &metadata.schema.name;
        let col = &metadata.schema.columns[*idx];
        let column_name = &col.name;

        conn.execute_batch(&indoc::formatdoc!(
          r#"
          DROP TRIGGER IF EXISTS __{table_name}__{column_name}__update_trigger;
          CREATE TRIGGER IF NOT EXISTS __{table_name}__{column_name}__update_trigger AFTER UPDATE ON "{table_name}"
            WHEN OLD."{column_name}" IS NOT NULL AND OLD."{column_name}" != NEW."{column_name}"
            BEGIN
              INSERT INTO _file_deletions (table_name, record_rowid, column_name, json) VALUES
                ('{table_name}', OLD._rowid_, '{column_name}', OLD."{column_name}");
            END;

          DROP TRIGGER IF EXISTS __{table_name}__{column_name}__delete_trigger;
          CREATE TRIGGER IF NOT EXISTS __{table_name}__{column_name}__delete_trigger AFTER DELETE ON "{table_name}"
            --FOR EACH ROW
            WHEN OLD."{column_name}" IS NOT NULL
            BEGIN
              INSERT INTO _file_deletions (table_name, record_rowid, column_name, json) VALUES
                ('{table_name}', OLD._rowid_, '{column_name}', OLD."{column_name}");
            END;
          "#)).await?;
      }
    }

    return Ok(table_metadata_map);
  }

  async fn build_views(
    conn: &trailbase_sqlite::Connection,
    tables: &[Table],
  ) -> Result<HashMap<String, Arc<ViewMetadata>>, TableLookupError> {
    let views = lookup_and_parse_all_view_schemas(conn, tables).await?;
    let build = |view: View| {
      // NOTE: we check during record API config validation that no temporary views are referenced.
      // if view.temporary {
      //   debug!("Temporary view: {}", view.name);
      // }

      return Some((view.name.clone(), Arc::new(ViewMetadata::new(view, tables))));
    };

    return Ok(views.into_iter().filter_map(build).collect());
  }

  // TODO: rename to get_table or split cache.
  pub fn get(&self, table_name: &str) -> Option<Arc<TableMetadata>> {
    self.state.read().tables.get(table_name).cloned()
  }

  pub fn get_view(&self, view_name: &str) -> Option<Arc<ViewMetadata>> {
    self.state.read().views.get(view_name).cloned()
  }

  pub(crate) fn tables(&self) -> Vec<TableMetadata> {
    return self
      .state
      .read()
      .tables
      .values()
      .map(|t| (**t).clone())
      .collect();
  }

  pub async fn invalidate_all(&self) -> Result<(), TableLookupError> {
    debug!("Rebuilding TableMetadataCache");
    let conn = &self.conn;

    let tables = lookup_and_parse_all_table_schemas(conn).await?;
    let table_map = Self::build_tables(conn, &tables).await?;
    let views = Self::build_views(conn, &tables).await?;

    *self.state.write() = TableMetadataCacheState {
      tables: table_map,
      views,
    };

    Ok(())
  }
}

impl std::fmt::Debug for TableMetadataCache {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let state = self.state.read();
    f.debug_struct("TableMetadataCache")
      .field("tables", &state.tables.keys())
      .field("views", &state.views.keys())
      .finish()
  }
}

#[derive(Debug, Error)]
pub enum TableLookupError {
  #[error("SQL2 error: {0}")]
  Sql(#[from] trailbase_sqlite::Error),
  #[error("SQL3 error: {0}")]
  FromSql(#[from] rusqlite::types::FromSqlError),
  #[error("Schema error: {0}")]
  Schema(#[from] SchemaError),
  #[error("Missing")]
  Missing,
  #[error("Sql parse error: {0}")]
  SqlParse(#[from] sqlite3_parser::lexer::sql::Error),
}

pub async fn lookup_and_parse_table_schema(
  conn: &trailbase_sqlite::Connection,
  table_name: &str,
) -> Result<Table, TableLookupError> {
  // Then get the actual table.
  let sql: String = conn
    .query_value(
      &format!("SELECT sql FROM {SQLITE_SCHEMA_TABLE} WHERE type = 'table' AND name = $1"),
      params!(table_name.to_string()),
    )
    .await?
    .ok_or_else(|| trailbase_sqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows))?;

  let Some(stmt) = sqlite3_parse_into_statement(&sql)? else {
    return Err(TableLookupError::Missing);
  };

  return Ok(stmt.try_into()?);
}

pub async fn lookup_and_parse_all_table_schemas(
  conn: &trailbase_sqlite::Connection,
) -> Result<Vec<Table>, TableLookupError> {
  // Then get the actual table.
  let rows = conn
    .query(
      &format!("SELECT sql FROM {SQLITE_SCHEMA_TABLE} WHERE type = 'table'"),
      (),
    )
    .await?;

  let mut tables: Vec<Table> = vec![];
  for row in rows.iter() {
    let sql: String = row.get(0)?;
    let Some(stmt) = sqlite3_parse_into_statement(&sql)? else {
      return Err(TableLookupError::Missing);
    };
    tables.push(stmt.try_into()?);
  }

  return Ok(tables);
}

fn sqlite3_parse_view(sql: &str, tables: &[Table]) -> Result<View, TableLookupError> {
  let mut parser = sqlite3_parser::lexer::sql::Parser::new(sql.as_bytes());
  match parser.next()? {
    None => Err(TableLookupError::Missing),
    Some(cmd) => {
      use sqlite3_parser::ast::Cmd;
      match cmd {
        Cmd::Stmt(stmt) => Ok(View::from(stmt, tables)?),
        Cmd::Explain(_) | Cmd::ExplainQueryPlan(_) => Err(TableLookupError::Missing),
      }
    }
  }
}

pub async fn lookup_and_parse_all_view_schemas(
  conn: &trailbase_sqlite::Connection,
  tables: &[Table],
) -> Result<Vec<View>, TableLookupError> {
  // Then get the actual table.
  let rows = conn
    .query(
      &format!("SELECT sql FROM {SQLITE_SCHEMA_TABLE} WHERE type = 'view'"),
      (),
    )
    .await?;

  let mut views: Vec<View> = vec![];
  for row in rows.iter() {
    let sql: String = row.get(0)?;
    views.push(sqlite3_parse_view(&sql, tables)?);
  }

  return Ok(views);
}

#[cfg(test)]
mod tests {
  use axum::extract::{Json, Path, Query, RawQuery, State};
  use serde_json::json;
  use trailbase_schema::json_schema::{build_json_schema_expanded, Expand, JsonSchemaMode};

  use crate::app_state::*;
  use crate::config::proto::{PermissionFlag, RecordApiConfig};
  use crate::records::list_records::list_records_handler;
  use crate::records::read_record::{read_record_handler, ReadRecordQuery};
  use crate::records::*;

  #[tokio::test]
  async fn test_expanded_foreign_key() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    conn
      .execute(
        "CREATE TABLE foreign_table (id INTEGER PRIMARY KEY) STRICT",
        (),
      )
      .await
      .unwrap();

    let table_name = "test_table";
    conn
      .execute(
        &format!(
          r#"CREATE TABLE {table_name} (
            id INTEGER PRIMARY KEY,
            fk INTEGER REFERENCES foreign_table(id)
          ) STRICT"#
        ),
        (),
      )
      .await
      .unwrap();

    state.table_metadata().invalidate_all().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("test_table_api".to_string()),
        table_name: Some(table_name.to_string()),
        acl_world: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
        expand: vec!["fk".to_string()],
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let test_table_metadata = state.table_metadata().get(table_name).unwrap();

    let (validator, schema) = build_json_schema_expanded(
      table_name,
      &test_table_metadata.schema.columns,
      JsonSchemaMode::Select,
      Some(Expand {
        tables: &state.table_metadata().tables(),
        foreign_key_columns: vec!["foreign_table"],
      }),
    )
    .unwrap();

    assert_eq!(
      schema,
      json!({
        "title": table_name,
        "type": "object",
        "properties": {
          "id": { "type": "integer" },
          "fk": { "$ref": "#/$defs/fk" },
        },
        "required": ["id"],
        "$defs": {
          "fk": {
            "type": "object",
            "properties": {
              "id" : { "type": "integer"},
              "data": {
                "title": "foreign_table",
                "type": "object",
                "properties": {
                  "id" : { "type": "integer" },
                },
                "required": ["id"],
              },
            },
            "required": ["id"],
          },
        },
      })
    );

    conn
      .execute("INSERT INTO foreign_table (id) VALUES (1);", ())
      .await
      .unwrap();

    conn
      .execute(
        &format!("INSERT INTO {table_name} (id, fk) VALUES (1, 1);"),
        (),
      )
      .await
      .unwrap();

    // Expansion of invalid column.
    {
      let response = read_record_handler(
        State(state.clone()),
        Path(("test_table_api".to_string(), "1".to_string())),
        Query(ReadRecordQuery {
          expand: Some("UNKNOWN".to_string()),
        }),
        None,
      )
      .await;

      assert!(response.is_err());

      let list_response = list_records_handler(
        State(state.clone()),
        Path("test_table_api".to_string()),
        RawQuery(Some("expand=UNKNOWN".to_string())),
        None,
      )
      .await;

      assert!(list_response.is_err());
    }

    // Not expanded
    {
      let expected = json!({
        "id": 1,
        "fk":{ "id": 1 },
      });

      let Json(value) = read_record_handler(
        State(state.clone()),
        Path(("test_table_api".to_string(), "1".to_string())),
        Query(ReadRecordQuery::default()),
        None,
      )
      .await
      .unwrap();

      validator.validate(&value).expect(&format!("{value}"));

      assert_eq!(expected, value);

      let Json(list_response) = list_records_handler(
        State(state.clone()),
        Path("test_table_api".to_string()),
        RawQuery(None),
        None,
      )
      .await
      .unwrap();

      assert_eq!(vec![expected.clone()], list_response.records);
      validator.validate(&list_response.records[0]).unwrap();
    }

    let expected = json!({
      "id": 1,
      "fk":{
        "id": 1,
        "data": {
          "id": 1,
        },
      },
    });

    {
      let Json(value) = read_record_handler(
        State(state.clone()),
        Path(("test_table_api".to_string(), "1".to_string())),
        Query(ReadRecordQuery {
          expand: Some("fk".to_string()),
        }),
        None,
      )
      .await
      .unwrap();

      validator.validate(&value).expect(&format!("{value}"));

      assert_eq!(expected, value);
    }

    {
      let Json(list_response) = list_records_handler(
        State(state.clone()),
        Path("test_table_api".to_string()),
        RawQuery(Some("expand=fk".to_string())),
        None,
      )
      .await
      .unwrap();

      assert_eq!(vec![expected.clone()], list_response.records);
      validator.validate(&list_response.records[0]).unwrap();
    }

    {
      let Json(list_response) = list_records_handler(
        State(state.clone()),
        Path("test_table_api".to_string()),
        RawQuery(Some("count=1&expand=fk".to_string())),
        None,
      )
      .await
      .unwrap();

      assert_eq!(Some(1), list_response.total_count);
      assert_eq!(vec![expected], list_response.records);
      validator.validate(&list_response.records[0]).unwrap();
    }
  }

  #[tokio::test]
  async fn test_expanded_with_multiple_foreign_keys() {
    let state = test_state(None).await.unwrap();

    let exec = {
      let conn = state.conn();
      move |sql: &str| {
        let conn = conn.clone();
        let owned = sql.to_owned();
        return async move { conn.execute(&owned, ()).await };
      }
    };

    exec("CREATE TABLE foreign_table0 (id INTEGER PRIMARY KEY) STRICT")
      .await
      .unwrap();
    exec("CREATE TABLE foreign_table1 (id INTEGER PRIMARY KEY) STRICT")
      .await
      .unwrap();

    let table_name = "test_table";
    exec(&format!(
      r#"CREATE TABLE {table_name} (
          id        INTEGER PRIMARY KEY,
          fk0       INTEGER REFERENCES foreign_table0(id),
          fk0_null  INTEGER REFERENCES foreign_table0(id),
          fk1       INTEGER REFERENCES foreign_table1(id)
        ) STRICT"#
    ))
    .await
    .unwrap();

    state.table_metadata().invalidate_all().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("test_table_api".to_string()),
        table_name: Some(table_name.to_string()),
        acl_world: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
        expand: vec!["fk0".to_string(), "fk1".to_string()],
        ..Default::default()
      },
    )
    .await
    .unwrap();

    exec("INSERT INTO foreign_table0 (id) VALUES (1);")
      .await
      .unwrap();
    exec("INSERT INTO foreign_table1 (id) VALUES (1);")
      .await
      .unwrap();

    exec(&format!(
      "INSERT INTO {table_name} (id, fk0, fk0_null, fk1) VALUES (1, 1, NULL, 1);"
    ))
    .await
    .unwrap();

    // Expand none
    {
      let Json(value) = read_record_handler(
        State(state.clone()),
        Path(("test_table_api".to_string(), "1".to_string())),
        Query(ReadRecordQuery { expand: None }),
        None,
      )
      .await
      .unwrap();

      let expected = json!({
        "id": 1,
        "fk0": { "id": 1 },
        "fk0_null": serde_json::Value::Null,
        "fk1": { "id": 1 },
      });

      assert_eq!(expected, value);

      let Json(list_response) = list_records_handler(
        State(state.clone()),
        Path("test_table_api".to_string()),
        RawQuery(None),
        None,
      )
      .await
      .unwrap();

      assert_eq!(vec![expected], list_response.records);
    }

    // Expand one
    {
      let expected = json!({
        "id": 1,
        "fk0": { "id": 1 },
        "fk0_null": serde_json::Value::Null,
        "fk1": {
          "id": 1,
          "data": {
            "id": 1,
          },
        },
      });

      let Json(value) = read_record_handler(
        State(state.clone()),
        Path(("test_table_api".to_string(), "1".to_string())),
        Query(ReadRecordQuery {
          expand: Some("fk1".to_string()),
        }),
        None,
      )
      .await
      .unwrap();

      assert_eq!(expected, value);

      let Json(list_response) = list_records_handler(
        State(state.clone()),
        Path("test_table_api".to_string()),
        RawQuery(Some("expand=fk1".to_string())),
        None,
      )
      .await
      .unwrap();

      assert_eq!(vec![expected], list_response.records);
    }

    // Expand all.
    {
      let expected = json!({
        "id": 1,
        "fk0": {
          "id": 1,
          "data": {
            "id": 1,
          },
        },
        "fk0_null": serde_json::Value::Null,
        "fk1": {
          "id": 1,
          "data": {
            "id": 1,
          },
        },
      });

      let Json(value) = read_record_handler(
        State(state.clone()),
        Path(("test_table_api".to_string(), "1".to_string())),
        Query(ReadRecordQuery {
          expand: Some("fk0,fk1".to_string()),
        }),
        None,
      )
      .await
      .unwrap();

      assert_eq!(expected, value);

      exec(&format!("INSERT INTO {table_name} (id) VALUES (2);"))
        .await
        .unwrap();

      let Json(list_response) = list_records_handler(
        State(state.clone()),
        Path("test_table_api".to_string()),
        RawQuery(Some("expand=fk0,fk1".to_string())),
        None,
      )
      .await
      .unwrap();

      assert_eq!(
        vec![
          json!({
            "id": 2,
            "fk0": serde_json::Value::Null,
            "fk0_null":  serde_json::Value::Null,
            "fk1":  serde_json::Value::Null,
          }),
          expected
        ],
        list_response.records
      );
    }
  }
}
