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
use crate::schema::{Column, ColumnDataType, ColumnOption, SchemaError, Table, View};

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
        let Some(schema) = trailbase_schema::registry::get_compiled_schema(name) else {
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
pub struct JsonMetadata {
  pub columns: Vec<Option<JsonColumnMetadata>>,

  // Contains both, 'std.FileUpload' and 'std.FileUpload'.
  file_column_indexes: Vec<usize>,
}

impl JsonMetadata {
  pub fn has_file_columns(&self) -> bool {
    return !self.file_column_indexes.is_empty();
  }

  fn from_table(table: &Table) -> Self {
    return Self::from_columns(&table.columns);
  }

  fn from_view(view: &View) -> Option<Self> {
    return view.columns.as_ref().map(|cols| Self::from_columns(cols));
  }

  fn from_columns(columns: &[Column]) -> Self {
    let columns: Vec<_> = columns.iter().map(build_json_metadata).collect();

    let file_column_indexes = find_file_column_indexes(&columns);

    return Self {
      columns,
      file_column_indexes,
    };
  }
}

/// A data class describing a sqlite Table and additional meta data useful for TrailBase.
///
/// An example of TrailBase idiosyncrasies are UUIDv7 columns, which are a bespoke concept.
#[derive(Debug, Clone)]
pub struct TableMetadata {
  pub schema: Table,

  /// If and which column on this table qualifies as a record PK column, i.e. integer or UUIDv7.
  pub record_pk_column: Option<usize>,
  /// If and which columns on this table reference _user(id).
  pub user_id_columns: Vec<usize>,
  /// Metadata for CHECK(json_schema()) columns.
  pub json_metadata: JsonMetadata,

  name_to_index: HashMap<String, usize>,
  // TODO: Add triggers once sqlparser supports a sqlite "CREATE TRIGGER" statements.
}

impl TableMetadata {
  /// Build a new TableMetadata instance containing TrailBase/RecordApi specific information.
  ///
  /// NOTE: The list of all tables is needed only to extract interger/UUIDv7 pk columns for foreign
  /// key relationships.
  pub(crate) fn new(table: Table, tables: &[Table]) -> Self {
    let name_to_index = HashMap::<String, usize>::from_iter(
      table
        .columns
        .iter()
        .enumerate()
        .map(|(index, col)| (col.name.clone(), index)),
    );

    let record_pk_column = find_record_pk_column_index(&table.columns, tables);
    let user_id_columns = find_user_id_foreign_key_columns(&table.columns);
    let json_metadata = JsonMetadata::from_table(&table);

    return TableMetadata {
      schema: table,
      name_to_index,
      record_pk_column,
      user_id_columns,
      json_metadata,
    };
  }

  #[inline]
  pub fn name(&self) -> &str {
    return &self.schema.name;
  }

  #[inline]
  pub fn column_index_by_name(&self, key: &str) -> Option<usize> {
    return self.name_to_index.get(key).copied();
  }

  #[inline]
  pub fn column_by_name(&self, key: &str) -> Option<(usize, &Column)> {
    let index = self.column_index_by_name(key)?;
    return Some((index, &self.schema.columns[index]));
  }
}

/// A data class describing a sqlite View and future, additional meta data useful for TrailBase.
#[derive(Debug, Clone)]
pub struct ViewMetadata {
  pub schema: View,

  name_to_index: HashMap<String, usize>,
  record_pk_column: Option<usize>,
  json_metadata: Option<JsonMetadata>,
}

impl ViewMetadata {
  /// Build a new ViewMetadata instance containing TrailBase/RecordApi specific information.
  ///
  /// NOTE: The list of all tables is needed only to extract interger/UUIDv7 pk columns for foreign
  /// key relationships.
  pub(crate) fn new(view: View, tables: &[Table]) -> Self {
    let name_to_index = if let Some(ref columns) = view.columns {
      HashMap::<String, usize>::from_iter(
        columns
          .iter()
          .enumerate()
          .map(|(index, col)| (col.name.clone(), index)),
      )
    } else {
      HashMap::<String, usize>::new()
    };

    let record_pk_column = view
      .columns
      .as_ref()
      .and_then(|c| find_record_pk_column_index(c, tables));
    let json_metadata = JsonMetadata::from_view(&view);

    return ViewMetadata {
      schema: view,
      name_to_index,
      record_pk_column,
      json_metadata,
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
  pub fn column_by_name(&self, key: &str) -> Option<(usize, &Column)> {
    let index = self.column_index_by_name(key)?;
    let cols = self.schema.columns.as_ref()?;
    return Some((index, &cols[index]));
  }
}

pub trait TableOrViewMetadata {
  fn record_pk_column(&self) -> Option<(usize, &Column)>;
  fn json_metadata(&self) -> Option<&JsonMetadata>;
  fn columns(&self) -> Option<&[Column]>;
}

impl TableOrViewMetadata for TableMetadata {
  fn columns(&self) -> Option<&[Column]> {
    return Some(&self.schema.columns);
  }

  fn json_metadata(&self) -> Option<&JsonMetadata> {
    return Some(&self.json_metadata);
  }

  fn record_pk_column(&self) -> Option<(usize, &Column)> {
    let index = self.record_pk_column?;
    return self.schema.columns.get(index).map(|c| (index, c));
  }
}

impl TableOrViewMetadata for ViewMetadata {
  fn columns(&self) -> Option<&[Column]> {
    return self.schema.columns.as_deref();
  }

  fn json_metadata(&self) -> Option<&JsonMetadata> {
    return self.json_metadata.as_ref();
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
      Regex::new(r#"(?smR)jsonschema\s*\(\s*[\['"](?<name>.*)[\]'"]\s*,.+?\)"#)
        .expect("infallible");
    static ref MATCHES_RE: Regex =
      Regex::new(r"(?smR)jsonschema_matches\s*\(.+?(?<pattern>\{.*\}).+?\)").expect("infallible");
  }

  if let Some(cap) = SCHEMA_RE.captures(check) {
    let name = &cap["name"];
    let Some(_schema) = trailbase_schema::registry::get_schema(name) else {
      let schemas: Vec<String> = trailbase_schema::registry::get_schemas()
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

pub(crate) fn find_file_column_indexes(
  json_column_metadata: &[Option<JsonColumnMetadata>],
) -> Vec<usize> {
  let mut indexes: Vec<usize> = vec![];
  for (index, column) in json_column_metadata.iter().enumerate() {
    if let Some(ref metadata) = column {
      match metadata {
        JsonColumnMetadata::SchemaName(name) if name == "std.FileUpload" => {
          indexes.push(index);
        }
        JsonColumnMetadata::SchemaName(name) if name == "std.FileUploads" => {
          indexes.push(index);
        }
        _ => {}
      };
    }
  }

  return indexes;
}

pub(crate) fn find_user_id_foreign_key_columns(columns: &[Column]) -> Vec<usize> {
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
      if let ColumnOption::Unique { is_primary, .. } = opt {
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
        static ref UUID_V7_RE: Regex = Regex::new(r"^is_uuid_v7\s*\(").expect("infallible");
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
              ColumnOption::Unique { is_primary, .. } if *is_primary => {
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
    let tables = lookup_and_parse_all_table_schemas(&conn).await?;
    let table_map = Self::build_tables(&conn, &tables).await?;
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
    tables: &[Table],
  ) -> Result<HashMap<String, Arc<TableMetadata>>, TableLookupError> {
    let table_metadata_map: HashMap<String, Arc<TableMetadata>> = tables
      .iter()
      .cloned()
      .map(|t: Table| (t.name.clone(), Arc::new(TableMetadata::new(t, tables))))
      .collect();

    // Install file column triggers. This ain't pretty, this might be better on construction and
    // schema changes.
    for metadata in table_metadata_map.values() {
      for idx in &metadata.json_metadata.file_column_indexes {
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

  pub fn get(&self, table_name: &str) -> Option<Arc<TableMetadata>> {
    self.state.tables.read().get(table_name).cloned()
  }

  pub fn get_view(&self, view_name: &str) -> Option<Arc<ViewMetadata>> {
    self.state.views.read().get(view_name).cloned()
  }

  pub async fn invalidate_all(&self) -> Result<(), TableLookupError> {
    debug!("Rebuilding TableMetadataCache");
    let conn = &self.state.conn;
    let tables = lookup_and_parse_all_table_schemas(conn).await?;
    let table_map = Self::build_tables(conn, &tables).await?;
    *self.state.tables.write() = table_map;
    *self.state.views.write() = Self::build_views(conn, &tables).await?;
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
  columns: &[Column],
  mode: JsonSchemaMode,
) -> Result<(Validator, serde_json::Value), JsonSchemaError> {
  return build_json_schema_recursive(table_or_view_name, columns, mode, None);
}

pub(crate) struct Expand<'a> {
  pub(crate) table_metadata: &'a TableMetadataCache,
  pub(crate) foreign_key_columns: Vec<&'a str>,
}

/// NOTE: Foreign keys can only reference tables not view, so the inline schemas don't need to be
/// able to reference views.
pub(crate) fn build_json_schema_recursive(
  table_or_view_name: &str,
  columns: &[Column],
  mode: JsonSchemaMode,
  expand: Option<Expand<'_>>,
) -> Result<(Validator, serde_json::Value), JsonSchemaError> {
  let mut properties = serde_json::Map::new();
  let mut defs = serde_json::Map::new();
  let mut required_cols: Vec<String> = vec![];

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
                let Some(schema) = trailbase_schema::registry::get_schema(&name) else {
                  return Err(JsonSchemaError::NotFound(name.to_string()));
                };
                defs.insert(col.name.clone(), schema.schema);
                found_def = true;
              }
              JsonColumnMetadata::Pattern(pattern) => {
                defs.insert(col.name.clone(), pattern.clone());
                found_def = true;
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
          referred_columns: _,
          ..
        } => {
          if let (Some(expand), JsonSchemaMode::Select) = (&expand, mode) {
            for metadata in &expand.foreign_key_columns {
              if metadata != foreign_table {
                continue;
              }

              // TODO: Implement nesting.
              let Some(table) = expand.table_metadata.get(foreign_table) else {
                warn!("Failed to find table: {foreign_table}");
                continue;
              };

              let Some((_idx, pk_column)) = table.record_pk_column() else {
                warn!("Missing pk column for table: {foreign_table}");
                continue;
              };

              let (_validator, schema) =
                build_json_schema(foreign_table, &table.schema.columns, mode)?;
              defs.insert(
                col.name.clone(),
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
              found_def = true;
            }
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
    } else {
      properties.insert(
        col.name.clone(),
        serde_json::json!({
          "type": column_data_type_to_json_type(col.data_type),
        }),
      );
    }
  }

  let schema = if defs.is_empty() {
    serde_json::json!({
      "title": table_or_view_name,
      "type": "object",
      "properties": serde_json::Value::Object(properties),
      "required": serde_json::json!(required_cols),
    })
  } else {
    serde_json::json!({
      "title": table_or_view_name,
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

#[cfg(test)]
mod tests {
  use axum::extract::{Json, Path, Query, RawQuery, State};
  use indoc::indoc;
  use serde_json::json;
  use trailbase_schema::FileUpload;

  use super::*;
  use crate::app_state::*;
  use crate::config::proto::{PermissionFlag, RecordApiConfig};
  use crate::records::list_records::list_records_handler;
  use crate::records::read_record::{read_record_handler, ReadRecordQuery};
  use crate::records::*;
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
          uuid::Uuid::now_v7(),
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
          "id": uuid::Uuid::now_v7().to_string(),
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
        "id": uuid::Uuid::now_v7().to_string(),
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

    let (validator, schema) = build_json_schema_recursive(
      table_name,
      &test_table_metadata.schema.columns,
      JsonSchemaMode::Select,
      Some(Expand {
        table_metadata: state.table_metadata(),
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
      assert_eq!(table_view.query, query);
      assert_eq!(table_view.temporary, false);

      let view_metadata = ViewMetadata::new(table_view, &[table.clone()]);

      let uuidv7_col = view_metadata.record_pk_column().unwrap();
      let columns = view_metadata.columns().unwrap();
      assert_eq!(columns.len(), 3);
      assert_eq!(columns[uuidv7_col.0].name, "id");
    }
  }
}
