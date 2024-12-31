use fallible_iterator::FallibleIterator;
use jsonschema::Validator;
use lazy_static::lazy_static;
use log::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlite3_parser::ast::Stmt;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use trailbase_sqlite::params;

use crate::constants::{SQLITE_SCHEMA_TABLE, USER_TABLE};
use crate::schema::{Column, ColumnDataType, ColumnOption, ForeignKey, SchemaError, Table, View};

// TODO: Can we merge this with trailbase_sqlite::schema::SchemaError?
#[derive(Debug, Clone, Error)]
pub enum JsonSchemaError {
  #[error("Schema compile error: {0}")]
  SchemaCompile(String),
  #[error("Validation error")]
  Validation,
  #[error("Schema not found: {0}")]
  NotFound(String),
  #[error("Json serialization error: {0}")]
  JsonSerialization(Arc<serde_json::Error>),
}

#[derive(Clone, Debug)]
pub enum JsonColumnMetadata {
  SchemaName(String),
  Pattern(serde_json::Value),
}

impl JsonColumnMetadata {
  pub fn validate(&self, value: &serde_json::Value) -> Result<(), JsonSchemaError> {
    match self {
      Self::SchemaName(name) => {
        let Some(schema) = trailbase_sqlite::schema::get_compiled_schema(name) else {
          return Err(JsonSchemaError::NotFound(name.to_string()));
        };
        schema
          .validate(value)
          .map_err(|_err| JsonSchemaError::Validation)?;
        return Ok(());
      }
      Self::Pattern(pattern) => {
        let schema =
          Validator::new(pattern).map_err(|err| JsonSchemaError::SchemaCompile(err.to_string()))?;
        if !schema.is_valid(value) {
          Err(JsonSchemaError::Validation)
        } else {
          Ok(())
        }
      }
    }
  }
}

#[derive(Debug, Clone)]
pub struct ColumnMetadata {
  pub json: Option<JsonColumnMetadata>,
}

/// A data class describing a sqlite Table and additional meta data useful for TrailBase.
///
/// An example of TrailBase idiosyncrasies are UUIDv7 columns, which are a bespoke concept.
#[derive(Debug, Clone)]
pub struct TableMetadata {
  pub schema: Table,

  metadata: Vec<ColumnMetadata>,
  name_to_index: HashMap<String, usize>,

  record_pk_column: Option<usize>,
  pub user_id_columns: Vec<usize>,
  pub file_upload_columns: Vec<usize>,
  pub file_uploads_columns: Vec<usize>,

  // Only non-composite keys.
  #[allow(unused)]
  foreign_ids: Vec<(usize, ForeignKey)>,
  // TODO: Add triggers once sqlparser supports a sqlite "CREATE TRIGGER" statements.
}

impl TableMetadata {
  /// Build a new TableMetadata instance containing TrailBase/RecordApi specific information.
  ///
  /// NOTE: The list of all tables is needed only to extract interger/UUIDv7 pk columns for foreign
  /// key relationships.
  pub(crate) fn new(table: Table, tables: &[Table]) -> Self {
    let mut foreign_ids: Vec<(usize, ForeignKey)> = vec![];
    let mut file_upload_columns: Vec<usize> = vec![];
    let mut file_uploads_columns: Vec<usize> = vec![];
    let mut name_to_index = HashMap::<String, usize>::new();

    let metadata: Vec<ColumnMetadata> = table
      .columns
      .iter()
      .enumerate()
      .map(|(index, col)| {
        name_to_index.insert(col.name.clone(), index);

        for opt in &col.options {
          if let ColumnOption::ForeignKey {
            foreign_table,
            referred_columns,
            on_delete,
            on_update,
          } = opt
          {
            foreign_ids.push((
              index,
              ForeignKey {
                name: None,
                foreign_table: foreign_table.clone(),
                columns: vec![col.name.clone()],
                referred_columns: referred_columns.clone(),
                on_delete: on_delete.clone(),
                on_update: on_update.clone(),
              },
            ));
          }
        }

        let json_metadata = build_json_metadata(col);
        if let Some(ref json_metadata) = json_metadata {
          match json_metadata {
            JsonColumnMetadata::SchemaName(name) if name == "std.FileUpload" => {
              file_upload_columns.push(index);
            }
            JsonColumnMetadata::SchemaName(name) if name == "std.FileUploads" => {
              file_uploads_columns.push(index);
            }
            _ => {}
          };
        }

        return ColumnMetadata {
          json: json_metadata,
        };
      })
      .collect();

    let record_pk_column = find_record_pk_column_index(&table.columns, tables);
    let user_id_columns = find_user_id_foreign_key_columns(&table.columns);

    return TableMetadata {
      schema: table,
      metadata,
      name_to_index,
      record_pk_column,
      user_id_columns,
      file_upload_columns,
      file_uploads_columns,
      foreign_ids,
    };
  }

  #[inline]
  pub fn name(&self) -> &str {
    &self.schema.name
  }

  #[inline]
  pub fn column_index_by_name(&self, key: &str) -> Option<usize> {
    self.name_to_index.get(key).copied()
  }

  #[inline]
  pub fn column_by_name(&self, key: &str) -> Option<(&Column, &ColumnMetadata)> {
    let index = self.column_index_by_name(key)?;
    return Some((&self.schema.columns[index], &self.metadata[index]));
  }
}

/// A data class describing a sqlite View and future, additional meta data useful for TrailBase.
#[derive(Debug, Clone)]
pub struct ViewMetadata {
  pub schema: View,

  metadata: Vec<ColumnMetadata>,
  name_to_index: HashMap<String, usize>,

  record_pk_column: Option<usize>,
}

impl ViewMetadata {
  /// Build a new ViewMetadata instance containing TrailBase/RecordApi specific information.
  ///
  /// NOTE: The list of all tables is needed only to extract interger/UUIDv7 pk columns for foreign
  /// key relationships.
  pub(crate) fn new(view: View, tables: &[Table]) -> Self {
    let mut name_to_index = HashMap::<String, usize>::new();
    let metadata: Vec<ColumnMetadata> = {
      if let Some(ref columns) = view.columns {
        columns
          .iter()
          .enumerate()
          .map(|(index, col)| {
            name_to_index.insert(col.name.clone(), index);
            return ColumnMetadata {
              json: build_json_metadata(col),
            };
          })
          .collect()
      } else {
        debug!("Building ViewMetadata for complex view thus missing column information.");
        vec![]
      }
    };

    let record_pk_column = view
      .columns
      .as_ref()
      .and_then(|c| find_record_pk_column_index(c, tables));

    return ViewMetadata {
      schema: view,
      metadata,
      name_to_index,
      record_pk_column,
    };
  }

  #[inline]
  pub fn name(&self) -> &str {
    &self.schema.name
  }

  #[inline]
  pub fn column_index_by_name(&self, key: &str) -> Option<usize> {
    self.name_to_index.get(key).copied()
  }

  #[inline]
  pub fn column_by_name(&self, key: &str) -> Option<(&Column, &ColumnMetadata)> {
    let index = self.column_index_by_name(key)?;
    let cols = self.schema.columns.as_ref()?;
    return Some((&cols[index], &self.metadata[index]));
  }
}

pub trait TableOrViewMetadata {
  // Used by RecordAPI.
  fn column_by_name(&self, key: &str) -> Option<(&Column, &ColumnMetadata)>;

  // Impl detail: only used by admin
  fn columns(&self) -> Option<Vec<Column>>;
  fn record_pk_column(&self) -> Option<(usize, &Column)>;
}

impl TableOrViewMetadata for TableMetadata {
  fn column_by_name(&self, key: &str) -> Option<(&Column, &ColumnMetadata)> {
    self.column_by_name(key)
  }

  fn columns(&self) -> Option<Vec<Column>> {
    Some(self.schema.columns.clone())
  }

  fn record_pk_column(&self) -> Option<(usize, &Column)> {
    let index = self.record_pk_column?;
    return self.schema.columns.get(index).map(|c| (index, c));
  }
}

impl TableOrViewMetadata for ViewMetadata {
  fn column_by_name(&self, key: &str) -> Option<(&Column, &ColumnMetadata)> {
    self.column_by_name(key)
  }

  fn columns(&self) -> Option<Vec<Column>> {
    return self.schema.columns.clone();
  }

  fn record_pk_column(&self) -> Option<(usize, &Column)> {
    let Some(columns) = &self.schema.columns else {
      return None;
    };
    let index = self.record_pk_column?;
    return columns.get(index).map(|c| (index, c));
  }
}

fn build_json_metadata(col: &Column) -> Option<JsonColumnMetadata> {
  for opt in &col.options {
    match extract_json_metadata(opt) {
      Ok(maybe) => {
        if let Some(jm) = maybe {
          return Some(jm);
        }
      }
      Err(err) => {
        error!("Failed to get JSON schema: {err}");
      }
    }
  }
  None
}

fn extract_json_metadata(
  opt: &ColumnOption,
) -> Result<Option<JsonColumnMetadata>, JsonSchemaError> {
  let ColumnOption::Check(check) = opt else {
    return Ok(None);
  };

  lazy_static! {
    static ref SCHEMA_RE: Regex =
      Regex::new(r#"(?smR)jsonschema\s*\(\s*[\['"](?<name>.*)[\]'"]\s*,.+?\)"#).unwrap();
    static ref MATCHES_RE: Regex =
      Regex::new(r"(?smR)jsonschema_matches\s*\(.+?(?<pattern>\{.*\}).+?\)").unwrap();
  }

  if let Some(cap) = SCHEMA_RE.captures(check) {
    let name = &cap["name"];
    let Some(_schema) = trailbase_sqlite::schema::get_schema(name) else {
      let schemas: Vec<String> = trailbase_sqlite::schema::get_schemas()
        .iter()
        .map(|s| s.name.clone())
        .collect();
      return Err(JsonSchemaError::NotFound(format!(
        "Json schema {name} not found in: {schemas:?}"
      )));
    };

    return Ok(Some(JsonColumnMetadata::SchemaName(name.to_string())));
  }

  if let Some(cap) = MATCHES_RE.captures(check) {
    let pattern = &cap["pattern"];
    let value = serde_json::from_str::<serde_json::Value>(pattern)
      .map_err(|err| JsonSchemaError::JsonSerialization(Arc::new(err)))?;
    return Ok(Some(JsonColumnMetadata::Pattern(value)));
  }

  return Ok(None);
}

fn find_user_id_foreign_key_columns(columns: &[Column]) -> Vec<usize> {
  let mut indexes: Vec<usize> = vec![];
  for (index, col) in columns.iter().enumerate() {
    for opt in &col.options {
      if let ColumnOption::ForeignKey {
        foreign_table,
        referred_columns,
        ..
      } = opt
      {
        if foreign_table == USER_TABLE && referred_columns.len() == 1 && referred_columns[0] == "id"
        {
          indexes.push(index);
        }
      }
    }
  }
  return indexes;
}

/// Finds suitable Integer or UUIDv7 primary key columns, if present.
///
/// Cursors require certain properties like a stable, time-sortable primary key.
fn find_record_pk_column_index(columns: &[Column], tables: &[Table]) -> Option<usize> {
  let primary_key_col_index = columns.iter().position(|col| {
    for opt in &col.options {
      if let ColumnOption::Unique { is_primary } = opt {
        return *is_primary;
      }
    }
    return false;
  });

  if let Some(index) = primary_key_col_index {
    let column = &columns[index];

    if column.data_type == ColumnDataType::Integer {
      // TODO: We should detect the "integer pk" desc case and at least warn:
      // https://www.sqlite.org/lang_createtable.html#rowid.
      return Some(index);
    }

    for opts in &column.options {
      lazy_static! {
        static ref UUID_V7_RE: Regex = Regex::new(r"^is_uuid_v7\s*\(").unwrap();
      }

      match &opts {
        // Check if the referenced column is a uuidv7 column.
        ColumnOption::ForeignKey {
          foreign_table,
          referred_columns,
          ..
        } => {
          let Some(referred_table) = tables.iter().find(|t| t.name == *foreign_table) else {
            error!("Failed to get foreign key schema for {foreign_table}");
            continue;
          };

          if referred_columns.len() != 1 {
            return None;
          }
          let referred_column = &referred_columns[0];

          let col = referred_table
            .columns
            .iter()
            .find(|c| c.name == *referred_column)?;

          let mut is_pk = false;
          for opt in &col.options {
            match opt {
              ColumnOption::Check(expr) if UUID_V7_RE.is_match(expr) => {
                return Some(index);
              }
              ColumnOption::Unique { is_primary } if *is_primary => {
                is_pk = true;
              }
              _ => {}
            }
          }

          if is_pk && col.data_type == ColumnDataType::Integer {
            return Some(index);
          }

          return None;
        }
        ColumnOption::Check(expr) if UUID_V7_RE.is_match(expr) => {
          return Some(index);
        }
        _ => {}
      }
    }
  }

  return None;
}

struct TableMetadataCacheState {
  conn: trailbase_sqlite::Connection,
  tables: parking_lot::RwLock<HashMap<String, Arc<TableMetadata>>>,
  views: parking_lot::RwLock<HashMap<String, Arc<ViewMetadata>>>,
}

#[derive(Clone)]
pub struct TableMetadataCache {
  state: Arc<TableMetadataCacheState>,
}

impl TableMetadataCache {
  pub async fn new(conn: trailbase_sqlite::Connection) -> Result<Self, TableLookupError> {
    let (table_map, tables) = Self::build_tables(&conn).await?;
    let views = Self::build_views(&conn, &tables).await?;

    return Ok(TableMetadataCache {
      state: Arc::new(TableMetadataCacheState {
        conn,
        tables: parking_lot::RwLock::new(table_map),
        views: parking_lot::RwLock::new(views),
      }),
    });
  }

  async fn build_tables(
    conn: &trailbase_sqlite::Connection,
  ) -> Result<(HashMap<String, Arc<TableMetadata>>, Vec<Table>), TableLookupError> {
    let tables = lookup_and_parse_all_table_schemas(conn).await?;
    let build = |table: &Table| {
      (
        table.name.clone(),
        Arc::new(TableMetadata::new(table.clone(), &tables)),
      )
    };

    return Ok((tables.iter().map(build).collect(), tables));
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

  pub fn get(&self, table_name: &str) -> Option<Arc<TableMetadata>> {
    self.state.tables.read().get(table_name).cloned()
  }

  pub fn get_view(&self, view_name: &str) -> Option<Arc<ViewMetadata>> {
    self.state.views.read().get(view_name).cloned()
  }

  pub async fn invalidate_all(&self) -> Result<(), TableLookupError> {
    debug!("Rebuilding TableMetadataCache");
    let (table_map, tables) = Self::build_tables(&self.state.conn).await?;
    *self.state.tables.write() = table_map;
    *self.state.views.write() = Self::build_views(&self.state.conn, &tables).await?;
    Ok(())
  }
}

impl std::fmt::Debug for TableMetadataCache {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("TableMetadataCache")
      .field("tables", &self.state.tables.read().keys())
      .field("views", &self.state.views.read().keys())
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
  let sql: String = crate::util::query_one_row(
    conn,
    &format!("SELECT sql FROM {SQLITE_SCHEMA_TABLE} WHERE type = 'table' AND name = $1"),
    params!(table_name.to_string()),
  )
  .await?
  .get(0)?;

  let Some(stmt) = sqlite3_parse_into_statement(&sql)? else {
    return Err(TableLookupError::Missing);
  };

  return Ok(stmt.try_into()?);
}

pub(crate) fn sqlite3_parse_into_statements(
  sql: &str,
) -> Result<Vec<Stmt>, sqlite3_parser::lexer::sql::Error> {
  use sqlite3_parser::ast::Cmd;

  // According to sqlite3_parser's docs they're working to remove panics in some edge cases.
  // Meanwhile we'll trap them here. We haven't seen any in practice yet.
  let outer_result = std::panic::catch_unwind(|| {
    let mut parser = sqlite3_parser::lexer::sql::Parser::new(sql.as_bytes());

    let mut statements: Vec<Stmt> = vec![];
    while let Some(cmd) = parser.next()? {
      match cmd {
        Cmd::Stmt(stmt) => {
          statements.push(stmt);
        }
        Cmd::Explain(_) | Cmd::ExplainQueryPlan(_) => {}
      }
    }
    return Ok(statements);
  });

  return match outer_result {
    Ok(inner_result) => inner_result,
    Err(_panic_err) => {
      error!("Parser panicked");
      return Err(sqlite3_parser::lexer::sql::Error::UnrecognizedToken(None));
    }
  };
}

pub(crate) fn sqlite3_parse_into_statement(
  sql: &str,
) -> Result<Option<Stmt>, sqlite3_parser::lexer::sql::Error> {
  use sqlite3_parser::ast::Cmd;

  // According to sqlite3_parser's docs they're working to remove panics in some edge cases.
  // Meanwhile we'll trap them here. We haven't seen any in practice yet.
  let outer_result = std::panic::catch_unwind(|| {
    let mut parser = sqlite3_parser::lexer::sql::Parser::new(sql.as_bytes());

    while let Some(cmd) = parser.next()? {
      match cmd {
        Cmd::Stmt(stmt) => {
          return Ok(Some(stmt));
        }
        Cmd::Explain(_) | Cmd::ExplainQueryPlan(_) => {}
      }
    }
    return Ok(None);
  });

  return match outer_result {
    Ok(inner_result) => inner_result,
    Err(_panic_err) => {
      error!("Parser panicked");
      return Err(sqlite3_parser::lexer::sql::Error::UnrecognizedToken(None));
    }
  };
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

/// Builds a JSON Schema definition for the given table.
///
/// NOTE: insert and select require different types to model default values, i.e. a column with a
/// default value is optional during insert but guaranteed during reads.
///
/// NOTE: We're not currently respecting the RecordApi `autofill_missing_user_id_columns`
/// setting. Not sure we should since this is more a feature for no-JS, HTTP-only apps, which
/// don't benefit from type-safety anyway.
pub fn build_json_schema(
  table_or_view_name: &str,
  metadata: &(dyn TableOrViewMetadata + Send + Sync),
  mode: JsonSchemaMode,
) -> Result<(Validator, serde_json::Value), JsonSchemaError> {
  let mut properties = serde_json::Map::new();
  let mut required_cols: Vec<String> = vec![];
  let mut defs = serde_json::Map::new();

  let Some(columns) = metadata.columns() else {
    return Err(JsonSchemaError::NotFound("".to_string()));
  };

  for col in columns {
    let mut found_def = false;
    let mut not_null = false;
    let mut default = false;

    for opt in &col.options {
      match opt {
        ColumnOption::NotNull => not_null = true,
        ColumnOption::Default(_) => default = true,
        ColumnOption::Check(check) => {
          if let Some(json_metadata) = extract_json_metadata(&ColumnOption::Check(check.clone()))? {
            match json_metadata {
              JsonColumnMetadata::SchemaName(name) => {
                let Some(schema) = trailbase_sqlite::schema::get_schema(&name) else {
                  return Err(JsonSchemaError::NotFound(name.to_string()));
                };
                defs.insert(col.name.clone(), schema.schema);
                found_def = true;
                break;
              }
              JsonColumnMetadata::Pattern(pattern) => {
                defs.insert(col.name.clone(), pattern.clone());
                found_def = true;
                break;
              }
            }
          }
        }
        ColumnOption::Unique { is_primary } => {
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

    if found_def {
      let name = &col.name;
      properties.insert(
        name.clone(),
        serde_json::json!({
          "$ref": format!("#/$defs/{name}")
        }),
      );
      continue;
    }

    properties.insert(
      col.name.clone(),
      serde_json::json!({
        "type": column_data_type_to_json_type(col.data_type),
      }),
    );
  }

  let schema = serde_json::json!({
    "title": table_or_view_name,
    "type": "object",
    "properties": serde_json::Value::Object(properties),
    "required": serde_json::Value::Array(required_cols.into_iter().map(serde_json::Value::String).collect()),
    "$defs":serde_json::Value::Object(defs),
  });

  return Ok((
    Validator::new(&schema).map_err(|err| JsonSchemaError::SchemaCompile(err.to_string()))?,
    schema,
  ));
}

#[cfg(test)]
mod tests {
  use indoc::indoc;
  use serde_json::json;
  use trailbase_sqlite::schema::FileUpload;

  use super::*;
  use crate::app_state::*;
  use crate::schema::ColumnOption;

  #[tokio::test]
  async fn test_parse_table_schema() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    let check = indoc! {r#"
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
      .await
      .unwrap();

    let insert = |col: &'static str, json: serde_json::Value| async move {
      conn
        .execute(
          &format!(
            "INSERT INTO test_table ({col}) VALUES ('{}')",
            json.to_string()
          ),
          (),
        )
        .await
    };

    assert!(insert("col2", json!({"name": 42})).await.unwrap() > 0);
    assert!(
      insert(
        "col1",
        serde_json::to_value(FileUpload::new(
          uuid::Uuid::new_v4(),
          Some("filename".to_string()),
          None,
          None
        ))
        .unwrap()
      )
      .await
      .unwrap()
        > 0
    );
    assert!(insert("col1", json!({"foo": "/foo"})).await.is_err());
    assert!(insert("col0", json!({"name": 42})).await.is_err());
    assert!(insert("col0", json!({"name": "Alice"})).await.is_err());
    assert!(
      insert("col0", json!({"name": "Alice", "age": 23}))
        .await
        .unwrap()
        > 0
    );
    assert!(insert(
      "col0",
      json!({"name": "Alice", "age": 23, "additional": 42})
    )
    .await
    .is_err());

    assert!(insert("col3", json!({"foo": "/foo"})).await.is_err());
    assert!(insert(
      "col3",
      json!({
          "id": uuid::Uuid::new_v4().to_string(),
          // Missing mime-type.
      })
    )
    .await
    .is_err());
    assert!(insert("col3", json!({"mime_type": "invalid"}))
      .await
      .is_err());
    assert!(insert(
      "col3",
      json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "mime_type": "image/png"
      })
    )
    .await
    .is_ok());

    let cnt: i64 = conn
      .query_row("SELECT COUNT(*) FROM test_table", ())
      .await
      .unwrap()
      .unwrap()
      .get(0)
      .unwrap();

    assert_eq!(cnt, 4);

    let table = lookup_and_parse_table_schema(conn, "test_table")
      .await
      .unwrap();
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
    let table_metadata = TableMetadata::new(table.clone(), &[table]);

    let (schema, _) = build_json_schema(
      table_metadata.name(),
      &table_metadata,
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

  #[test]
  fn test_parse_alter_table() {
    let sql = "ALTER TABLE foo RENAME TO bar";
    sqlite3_parse_into_statements(sql).unwrap();
  }

  #[test]
  fn test_parse_create_view() {
    let table_name = "table_name";
    let table_sql = format!(
      r#"
      CREATE TABLE {table_name} (
          id                           BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(id)) DEFAULT (uuid_v7()),
          col0                         TEXT NOT NULL DEFAULT '',
          col1                         BLOB NOT NULL,
          hidden                       INTEGER DEFAULT 42
      ) STRICT;"#
    );

    let create_table_statement = sqlite3_parse_into_statement(&table_sql).unwrap().unwrap();

    let table: Table = create_table_statement.try_into().unwrap();

    {
      let view_name = "view_name";
      let query = format!("SELECT col0, col1 FROM {table_name}");
      let view_sql = format!("CREATE VIEW {view_name} AS {query}");
      let create_view_statement = sqlite3_parse_into_statement(&view_sql).unwrap().unwrap();

      let table_view = View::from(create_view_statement, &[table.clone()]).unwrap();

      assert_eq!(table_view.name, view_name);
      assert_eq!(table_view.query, query);
      assert_eq!(table_view.temporary, false);

      let view_columns = table_view.columns.as_ref().unwrap();

      assert_eq!(view_columns.len(), 2);
      assert_eq!(view_columns[0].name, "col0");
      assert_eq!(view_columns[0].data_type, ColumnDataType::Text);

      assert_eq!(view_columns[1].name, "col1");
      assert_eq!(view_columns[1].data_type, ColumnDataType::Blob);

      let view_metadata = ViewMetadata::new(table_view, &[table.clone()]);

      assert!(view_metadata.record_pk_column().is_none());
      assert_eq!(view_metadata.columns().as_ref().unwrap().len(), 2);
    }

    {
      let view_name = "view_name";
      let query = format!("SELECT id, col0, col1 FROM {table_name}");
      let view_sql = format!("CREATE VIEW {view_name} AS {query}");
      let create_view_statement = sqlite3_parse_into_statement(&view_sql).unwrap().unwrap();

      let table_view = View::from(create_view_statement, &[table.clone()]).unwrap();

      assert_eq!(table_view.name, view_name);
      //FIXME:
      // assert_eq!(table_view.query, query);
      assert_eq!(table_view.temporary, false);

      let view_metadata = ViewMetadata::new(table_view, &[table.clone()]);

      let uuidv7_col = view_metadata.record_pk_column().unwrap();
      let columns = view_metadata.columns().unwrap();
      assert_eq!(columns.len(), 3);
      assert_eq!(columns[uuidv7_col.0].name, "id");
    }
  }
}
