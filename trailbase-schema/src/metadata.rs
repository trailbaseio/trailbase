use jsonschema::Validator;
use lazy_static::lazy_static;
use log::*;
use regex::Regex;
use sqlite3_parser::ast::JoinType;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use thiserror::Error;

use crate::sqlite::{
  Column, ColumnDataType, ColumnMapping, ColumnOption, QualifiedName, Table, View,
};

// TODO: Can we merge this with crate::sqlite::SchemaError?
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

#[derive(Clone, Debug, PartialEq)]
pub enum JsonColumnMetadata {
  SchemaName(String),
  Pattern(serde_json::Value),
}

impl JsonColumnMetadata {
  pub fn validate(&self, value: &serde_json::Value) -> Result<(), JsonSchemaError> {
    match self {
      Self::SchemaName(name) => {
        let Some(schema) = crate::registry::get_compiled_schema(name) else {
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

#[derive(Debug, Clone, PartialEq)]
pub struct JsonMetadata {
  pub columns: Vec<Option<JsonColumnMetadata>>,

  // Contains both, 'std.FileUpload' and 'std.FileUpload'.
  file_column_indexes: Vec<usize>,
}

impl JsonMetadata {
  pub fn has_file_columns(&self) -> bool {
    return !self.file_column_indexes.is_empty();
  }

  /// Contains both, 'std.FileUpload' and 'std.FileUpload'.
  pub fn file_column_indexes(&self) -> &[usize] {
    return &self.file_column_indexes;
  }

  fn from_table(table: &Table) -> Self {
    return Self::from_columns(&table.columns);
  }

  fn from_columns(columns: &[Column]) -> Self {
    let columns: Vec<_> = columns.iter().map(build_json_metadata).collect();

    return Self {
      file_column_indexes: find_file_column_indexes(&columns),
      columns,
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
  pub fn new(table: Table, tables: &[Table], user_table_name: &str) -> Self {
    let name_to_index = HashMap::<String, usize>::from_iter(
      table
        .columns
        .iter()
        .enumerate()
        .map(|(index, col)| (col.name.clone(), index)),
    );

    let record_pk_column = find_record_pk_column_index_for_table(&table, tables);
    let user_id_columns = find_user_id_foreign_key_columns(&table.columns, user_table_name);
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
  pub fn name(&self) -> &QualifiedName {
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

// Implement `PartialEq`, `Hash`, and `Borrow` for TableMetadata based on fully qualified name for
// use in HashSet.
impl PartialEq for TableMetadata {
  fn eq(&self, other: &Self) -> bool {
    return self.schema.name == other.schema.name;
  }
}

impl Eq for TableMetadata {}

// Implement `PartialEq`, `Hash`, and `Borrow` for TableMetadata based on fully qualified name for
// use in HashSet.
impl Hash for TableMetadata {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.schema.name.hash(state);
  }
}

// Implement `PartialEq`, `Hash`, and `Borrow` for TableMetadata based on fully qualified name for
// use in HashSet.
impl Borrow<QualifiedName> for TableMetadata {
  fn borrow(&self) -> &QualifiedName {
    return &self.schema.name;
  }
}

// Implement `PartialEq`, `Hash`, and `Borrow` for TableMetadata based on fully qualified name for
// use in HashSet.
impl Borrow<QualifiedName> for Arc<TableMetadata> {
  fn borrow(&self) -> &QualifiedName {
    return &self.schema.name;
  }
}

/// A data class describing a sqlite View and future, additional meta data useful for TrailBase.
#[derive(Debug, Clone)]
pub struct ViewMetadata {
  pub schema: View,

  // QUESTION: Why do we have copy of the columns here? Right now it's duplicate from `.schema`.
  // This probably only exists because we have a trait impl that returns Option<&[Column]>.
  columns: Option<Vec<Column>>,

  name_to_index: HashMap<String, usize>,
  record_pk_column: Option<usize>,
  json_metadata: Option<JsonMetadata>,
}

impl ViewMetadata {
  /// Build a new ViewMetadata instance containing TrailBase/RecordApi specific information.
  ///
  /// NOTE: The list of all tables is needed only to extract interger/UUIDv7 pk columns for foreign
  /// key relationships.
  pub fn new(view: View, tables: &[Table]) -> Self {
    return match view.column_mapping {
      Some(ref column_mapping) => {
        let columns: Vec<Column> = column_mapping
          .columns
          .iter()
          .map(|m| m.column.clone())
          .collect();

        let name_to_index = HashMap::<String, usize>::from_iter(
          columns
            .iter()
            .enumerate()
            .map(|(index, col)| (col.name.clone(), index)),
        );

        ViewMetadata {
          name_to_index,
          json_metadata: Some(JsonMetadata::from_columns(&columns)),
          columns: Some(columns),
          record_pk_column: find_record_pk_column_index_for_view(column_mapping, tables),
          schema: view,
        }
      }
      None => ViewMetadata {
        name_to_index: HashMap::<String, usize>::default(),
        columns: None,
        record_pk_column: None,
        json_metadata: None,
        schema: view,
      },
    };
  }

  #[inline]
  pub fn name(&self) -> &QualifiedName {
    &self.schema.name
  }

  #[inline]
  pub fn columns(&self) -> Option<&[Column]> {
    return self.columns.as_deref();
  }

  #[inline]
  pub fn column_index_by_name(&self, key: &str) -> Option<usize> {
    self.name_to_index.get(key).copied()
  }

  #[inline]
  pub fn column_by_name(&self, key: &str) -> Option<(usize, &Column)> {
    let index = self.column_index_by_name(key)?;
    let mapping = self.schema.column_mapping.as_ref()?;
    return Some((index, &mapping.columns[index].column));
  }
}

// Implement `PartialEq`, `Hash`, and `Borrow` for TableMetadata based on fully qualified name for
// use in HashSet.
impl PartialEq for ViewMetadata {
  fn eq(&self, other: &Self) -> bool {
    return self.schema.name == other.schema.name;
  }
}

impl Eq for ViewMetadata {}

// Implement `PartialEq`, `Hash`, and `Borrow` for TableMetadata based on fully qualified name for
// use in HashSet.
impl Hash for ViewMetadata {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.schema.name.hash(state);
  }
}

// Implement `PartialEq`, `Hash`, and `Borrow` for TableMetadata based on fully qualified name for
// use in HashSet.
impl Borrow<QualifiedName> for ViewMetadata {
  fn borrow(&self) -> &QualifiedName {
    return &self.schema.name;
  }
}

// Implement `PartialEq`, `Hash`, and `Borrow` for TableMetadata based on fully qualified name for
// use in HashSet.
impl Borrow<QualifiedName> for Arc<ViewMetadata> {
  fn borrow(&self) -> &QualifiedName {
    return &self.schema.name;
  }
}

pub trait TableOrViewMetadata {
  fn qualified_name(&self) -> &QualifiedName;
  fn record_pk_column(&self) -> Option<(usize, &Column)>;
  fn json_metadata(&self) -> Option<&JsonMetadata>;
  fn columns(&self) -> Option<&[Column]>;
}

impl TableOrViewMetadata for TableMetadata {
  fn qualified_name(&self) -> &QualifiedName {
    return self.name();
  }

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
  fn qualified_name(&self) -> &QualifiedName {
    return self.name();
  }

  fn columns(&self) -> Option<&[Column]> {
    return self.columns.as_deref();
  }

  fn json_metadata(&self) -> Option<&JsonMetadata> {
    return self.json_metadata.as_ref();
  }

  fn record_pk_column(&self) -> Option<(usize, &Column)> {
    let Some(columns) = &self.columns else {
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

pub fn extract_json_metadata(
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
    let Some(_schema) = crate::registry::get_schema(name) else {
      let schemas: Vec<String> = crate::registry::get_schemas()
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

pub fn find_file_column_indexes(json_column_metadata: &[Option<JsonColumnMetadata>]) -> Vec<usize> {
  let mut indexes: Vec<usize> = vec![];

  for (index, column) in json_column_metadata.iter().enumerate() {
    if let Some(metadata) = column {
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

pub fn find_user_id_foreign_key_columns(columns: &[Column], user_table_name: &str) -> Vec<usize> {
  let mut indexes: Vec<usize> = vec![];
  for (index, col) in columns.iter().enumerate() {
    for opt in &col.options {
      if let ColumnOption::ForeignKey {
        foreign_table,
        referred_columns,
        ..
      } = opt
      {
        if foreign_table == user_table_name
          && referred_columns.len() == 1
          && referred_columns[0] == "id"
        {
          indexes.push(index);
        }
      }
    }
  }
  return indexes;
}

pub(crate) fn is_pk_column(column: &Column) -> bool {
  for opt in &column.options {
    if let ColumnOption::Unique { is_primary, .. } = opt {
      return *is_primary;
    }
  }
  return false;
}

fn is_suitable_record_pk_column(column: &Column, tables: &[Table]) -> bool {
  if !is_pk_column(column) {
    return false;
  }

  return match column.data_type {
    ColumnDataType::Integer => {
      // TODO: We should detect the "integer pk" desc case and at least warn:
      // https://www.sqlite.org/lang_createtable.html#rowid.
      true
    }
    ColumnDataType::Blob => {
      lazy_static! {
        static ref UUID_CHECK_RE: Regex =
          Regex::new(r"^is_uuid(|_v7|_v4)\s*\(").expect("infallible");
      }

      for opts in &column.options {
        match opts {
          // Check the column itself is a UUID column.
          ColumnOption::Check(expr) if UUID_CHECK_RE.is_match(expr) => return true,
          // Or that a referenced column is a UUID column.
          ColumnOption::ForeignKey {
            foreign_table,
            referred_columns,
            ..
          } => {
            let referred_column = {
              if referred_columns.len() != 1 {
                return false;
              }
              &referred_columns[0]
            };

            // NOTE: Foreign keys cannot cross database boundaries, we can therefore compare by
            // unqualified name.
            let Some(referred_table) = tables.iter().find(|t| t.name.name == *foreign_table) else {
              warn!("Failed to get foreign key schema for {foreign_table}");
              return false;
            };

            let Some(foreign_column) = referred_table
              .columns
              .iter()
              .find(|c| c.name == *referred_column)
            else {
              return false;
            };

            for opt in &foreign_column.options {
              match opt {
                ColumnOption::Check(expr) if UUID_CHECK_RE.is_match(expr) => return true,
                _ => {}
              }
            }
          }
          _ => {}
        }
      }

      false
    }
    _ => false,
  };
}

/// Finds suitable Integer or UUIDv7/UUIDv4 primary key columns, if present.
///
/// Cursors require certain properties like a stable, time-sortable primary key.
fn find_record_pk_column_index_for_table(table: &Table, tables: &[Table]) -> Option<usize> {
  if table.strict {
    for (index, column) in table.columns.iter().enumerate() {
      if is_suitable_record_pk_column(column, tables) {
        return Some(index);
      }
    }
  }
  return None;
}

fn find_record_pk_column_index_for_view(
  column_mapping: &ColumnMapping,
  tables: &[Table],
) -> Option<usize> {
  if let Some(group_by_index) = column_mapping.group_by {
    let column = &column_mapping.columns[group_by_index];
    if is_suitable_record_pk_column(&column.column, tables) {
      return Some(group_by_index);
    }
    return None;
  }

  // NOTE: We could be smarter here. It's quite tricky to say with a set of arbitrary joins, which
  // (integer, UUID) columns end up being unique afterwards. Rely on explicit GROUP BY instead.
  let mask = JoinType::RIGHT | JoinType::CROSS | JoinType::NATURAL;
  for join_type in &column_mapping.joins {
    if join_type & mask.bits() != 0 {
      warn!("Only LEFT and INNER JOINS supported yet, got: {join_type:?}");
      return None;
    }
  }

  for (index, mapped_column) in column_mapping.columns.iter().enumerate() {
    if is_suitable_record_pk_column(&mapped_column.column, tables) {
      return Some(index);
    }
  }
  return None;
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;

  use super::*;
  use crate::parse::parse_into_statement;
  use crate::sqlite::{SchemaError, Table};

  fn parse_create_table(create_table_sql: &str) -> Table {
    let create_table_statement = parse_into_statement(create_table_sql).unwrap().unwrap();
    return create_table_statement.try_into().unwrap();
  }

  fn parse_create_view(create_view_sql: &str, tables: &[Table]) -> Result<View, SchemaError> {
    let create_view_statement = parse_into_statement(create_view_sql).unwrap().unwrap();
    return View::from(create_view_statement, tables);
  }

  #[test]
  fn test_find_record_pk_column_index_for_table() {
    let table = parse_create_table("CREATE TABLE t (id INTEGER PRIMARY KEY) STRICT");
    let tables = [table.clone()];
    assert_eq!(
      Some(0),
      find_record_pk_column_index_for_table(&table, &tables)
    );

    let table = parse_create_table(
      r#"
        CREATE TABLE t (
            value     TEXT,
            id        BLOB PRIMARY KEY NOT NULL CHECK(is_uuid(id))
        ) STRICT;
      "#,
    );

    let tables = [table.clone()];
    assert_eq!(
      Some(1),
      find_record_pk_column_index_for_table(&table, &tables)
    );

    let non_strict_table = parse_create_table(
      r#"
        CREATE TABLE t (
            value     TEXT,
            id        BLOB PRIMARY KEY NOT NULL CHECK(is_uuid(id))
        );
      "#,
    );
    let tables = [non_strict_table.clone()];
    assert_eq!(
      None,
      find_record_pk_column_index_for_table(&non_strict_table, &tables)
    );
  }

  #[test]
  fn test_parse_create_view() {
    let table = parse_create_table(
      r#"
        CREATE TABLE table0 (
            id               BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(id)) DEFAULT (uuid_v7()),
            col0             TEXT NOT NULL DEFAULT '',
            col1             BLOB NOT NULL,
            hidden           INTEGER DEFAULT 42
        ) STRICT;
      "#,
    );

    let tables = [table.clone()];
    let metadata = TableMetadata::new(table, &tables, "_user");

    assert_eq!("table0", metadata.name().name);
    assert_eq!("col1", metadata.columns().unwrap()[2].name);
    assert_eq!(1, *metadata.name_to_index.get("col0").unwrap());

    {
      let table_view = parse_create_view(
        "CREATE VIEW view0 AS SELECT col0, col1 FROM table0",
        &tables,
      )
      .unwrap();
      assert_eq!(table_view.name.name, "view0");
      assert_eq!(table_view.query, "SELECT col0, col1 FROM table0");
      assert_eq!(table_view.temporary, false);

      let view_columns = &table_view.column_mapping.as_ref().unwrap().columns;

      assert_eq!(view_columns.len(), 2);
      assert_eq!(view_columns[0].column.name, "col0");
      assert_eq!(view_columns[0].column.data_type, ColumnDataType::Text);

      assert_eq!(view_columns[1].column.name, "col1");
      assert_eq!(view_columns[1].column.data_type, ColumnDataType::Blob);

      let view_metadata = ViewMetadata::new(table_view, &tables);

      assert!(view_metadata.record_pk_column().is_none());
      assert_eq!(view_metadata.columns().as_ref().unwrap().len(), 2);
    }

    {
      let query = "SELECT id, col0, col1 FROM table0";
      let table_view =
        parse_create_view(&format!("CREATE VIEW view0 AS {query}"), &tables).unwrap();

      assert_eq!(table_view.name.name, "view0");
      assert_eq!(table_view.query, query);
      assert_eq!(table_view.temporary, false);

      let view_metadata = ViewMetadata::new(table_view, &tables);

      let uuidv7_col = view_metadata.record_pk_column().unwrap();
      let columns = view_metadata.columns().unwrap();
      assert_eq!(columns.len(), 3);
      assert_eq!(columns[uuidv7_col.0].name, "id");
    }
  }

  #[test]
  fn test_parse_create_view_with_subquery() {
    let table_a = parse_create_table(
      "CREATE TABLE a (id INTEGER PRIMARY KEY, data TEXT NOT NULL DEFAULT '') STRICT",
    );

    let tables = [table_a];

    {
      let view = parse_create_view(
        "CREATE VIEW view0 AS SELECT * FROM (SELECT * FROM a);",
        &tables,
      )
      .unwrap();
      let view_columns = &view.column_mapping.as_ref().unwrap().columns;

      assert_eq!(view_columns.len(), 2);
      assert_eq!(view_columns[0].column.name, "id");
      assert_eq!(view_columns[0].column.data_type, ColumnDataType::Integer);

      assert_eq!(view_columns[1].column.name, "data");
      assert_eq!(view_columns[1].column.data_type, ColumnDataType::Text);

      let metadata = ViewMetadata::new(view, &tables);
      let (pk_index, pk_col) = metadata.record_pk_column().unwrap();
      assert_eq!(pk_index, 0);
      assert_eq!(pk_col.name, "id");
    }

    {
      let view = parse_create_view(
        "CREATE VIEW view0 AS SELECT id FROM (SELECT * FROM a);",
        &tables,
      )
      .unwrap();
      let view_columns = &view.column_mapping.as_ref().unwrap().columns;
      assert_eq!(view_columns.len(), 1);
      assert_eq!(view_columns[0].column.name, "id");
      assert_eq!(view_columns[0].column.data_type, ColumnDataType::Integer);

      let metadata = ViewMetadata::new(view, &tables);
      let (pk_index, pk_col) = metadata.record_pk_column().unwrap();
      assert_eq!(pk_index, 0);
      assert_eq!(pk_col.name, "id");
    }

    {
      let view = parse_create_view(
        "CREATE VIEW view0 AS SELECT x.id FROM (SELECT * FROM a) AS x;",
        &tables,
      )
      .unwrap();
      let view_columns = &view.column_mapping.as_ref().unwrap().columns;
      assert_eq!(view_columns.len(), 1);
      assert_eq!(view_columns[0].column.name, "id");
      assert_eq!(view_columns[0].column.data_type, ColumnDataType::Integer);

      let metadata = ViewMetadata::new(view, &tables);
      let (pk_index, pk_col) = metadata.record_pk_column().unwrap();
      assert_eq!(pk_index, 0);
      assert_eq!(pk_col.name, "id");
    }

    {
      // JOIN on a SELECT is not suitable for APIs. They're cross-producty nature spoils PKs.
      let view = parse_create_view(
        "CREATE VIEW view0 AS SELECT x.id, y.id FROM (SELECT * FROM a) AS x, (SELECT * FROM a) AS y;",
        &tables,
      ).unwrap();
      assert!(view.column_mapping.is_none());
    }
  }

  #[test]
  fn test_parse_create_view_with_joins() {
    let table_a = parse_create_table(
      "CREATE TABLE a (id INTEGER PRIMARY KEY, data TEXT NOT NULL DEFAULT '') STRICT",
    );
    let table_b = parse_create_table(
      r#"
          CREATE TABLE b (
            id INTEGER PRIMARY KEY,
            fk INTEGER NOT NULL REFERENCES a(id)
          ) STRICT"#,
    );

    let tables = [table_a, table_b];

    {
      // LEFT JOIN
      let view = parse_create_view(
        "CREATE VIEW view0 AS SELECT a.data, b.fk, a.id FROM a AS a LEFT JOIN b AS b ON a.id = b.fk;",
        &tables,
      ).unwrap();
      let view_columns = &view.column_mapping.as_ref().unwrap().columns;

      assert_eq!(view_columns.len(), 3);
      assert_eq!(view_columns[2].column.name, "id");
      assert_eq!(view_columns[2].column.data_type, ColumnDataType::Integer);

      assert_eq!(view_columns[0].column.name, "data");
      assert_eq!(view_columns[0].column.data_type, ColumnDataType::Text);

      assert_eq!(view_columns[1].column.name, "fk");
      assert_eq!(view_columns[1].column.data_type, ColumnDataType::Integer);

      let metadata = ViewMetadata::new(view, &tables);
      let (pk_index, pk_col) = metadata.record_pk_column().unwrap();
      assert_eq!(pk_index, 2);
      assert_eq!(pk_col.name, "id");
    }

    {
      // JOINs
      for (join_type, expected) in [
        ("LEFT", Some(1)),
        ("INNER", Some(1)),
        ("RIGHT", None),
        ("CROSS", None),
      ] {
        let view = parse_create_view(
          &format!(
            "CREATE VIEW view0 AS SELECT a.data, a.id FROM a AS a {join_type} JOIN b AS b ON a.id = b.fk;"
          ),
          &tables,
        )
        .unwrap();

        let metadata = ViewMetadata::new(view, &tables);
        assert_eq!(
          expected,
          metadata.record_pk_column().map(|c| c.0),
          "{join_type}"
        );
      }
    }
  }

  #[test]
  fn test_parse_create_view_with_group_by() {
    let table_a = parse_create_table(
      "CREATE TABLE a (id INTEGER PRIMARY KEY, data TEXT NOT NULL DEFAULT '') STRICT",
    );
    let table_b = parse_create_table(
      r#"
          CREATE TABLE b (
            id INTEGER PRIMARY KEY,
            fk INTEGER NOT NULL REFERENCES a(id)
          ) STRICT"#,
    );

    let tables = [table_a, table_b];

    {
      // JOIN on a SELECT is not suitable for APIs. They're cross-producty nature spoils PKs.
      {
        for (i, sql) in [
          "CREATE VIEW v AS SELECT data, a.id AS z FROM a RIGHT JOIN b ON a.id = b.id GROUP BY z;",
          "CREATE VIEW v AS SELECT data, x.id AS z FROM a AS x RIGHT JOIN b ON x.id = b.id GROUP BY z;",
          "CREATE VIEW v AS SELECT data, x.id AS z FROM a x RIGHT JOIN b ON x.id = b.id GROUP BY z;",
        ].iter().enumerate() {
          let view = parse_create_view(sql, &tables).unwrap();
          assert!(view.column_mapping.is_some(), "{i}: {sql}");

          let metadata = ViewMetadata::new(view, &tables);
          assert_eq!(Some(1), metadata.record_pk_column().map(|c| c.0));
        }
      }

      {
        let view = parse_create_view(
          "CREATE VIEW v AS SELECT a.data, a.id FROM a RIGHT JOIN b ON a.id = b.id GROUP BY a.id;",
          &tables,
        )
        .unwrap();

        let metadata = ViewMetadata::new(view, &tables);
        assert_eq!(Some(1), metadata.record_pk_column().map(|c| c.0));
      }
    }
  }

  #[test]
  fn test_parse_create_view_from_issue_99() {
    let authors_table = parse_create_table(
      "
        CREATE TABLE authors (
          id INTEGER PRIMARY KEY,
          name TEXT NOT NULL,
          age INTEGER DEFAULT NULL
        ) STRICT;
      ",
    );
    let posts_table = parse_create_table(
      "
        CREATE TABLE posts (
          id INTEGER PRIMARY KEY,
          author INTEGER DEFAULT NULL REFERENCES persons(id),
          title TEXT NOT NULL
        ) STRICT;
      ",
    );

    let tables = [authors_table, posts_table];

    {
      let view = parse_create_view(
        "
            CREATE VIEW authors_view_posts AS
              SELECT authors.*, CAST(MAX(age) AS INTEGER) AS age FROM authors authors
                  INNER JOIN posts posts ON posts.author = authors.id
              GROUP BY authors.id;
          ",
        &tables,
      )
      .unwrap();

      let metadata = ViewMetadata::new(view, &tables);
      assert_eq!(Some(0), metadata.record_pk_column().map(|c| c.0));
    }

    {
      // And without explicit cast for well-known built-in "MAX".
      let view = parse_create_view(
        "
            CREATE VIEW authors_view_posts AS
              SELECT authors.*, MAX(age) FROM authors authors
                  INNER JOIN posts posts ON posts.author = authors.id
              GROUP BY authors.id;
          ",
        &tables,
      )
      .unwrap();

      let metadata = ViewMetadata::new(view, &tables);
      assert_eq!(Some(0), metadata.record_pk_column().map(|c| c.0));
    }
  }

  #[test]
  fn test_metadata_hash_set_by_name() {
    let table_name = QualifiedName {
      name: "table_name".to_string(),
      database_schema: Some("main".to_string()),
    };
    let table = parse_create_table(&format!(
      "CREATE TABLE {table_name} (id INTEGER PRIMARY KEY) STRICT",
      table_name = table_name.escaped_string()
    ));
    let tables = [table.clone()];

    let table_metadata = TableMetadata::new(table.clone(), &tables, "_user");

    let mut table_set = HashSet::<TableMetadata>::new();

    assert!(table_set.insert(table_metadata.clone()));
    assert!(table_set.get(&table_name).is_some());
    assert_eq!(
      table_set.get(&QualifiedName::parse("table_name").unwrap()),
      Some(&table_metadata)
    );

    // Test Arc<views>:
    let view_name = QualifiedName {
      name: "view_name".to_string(),
      database_schema: Some("main".to_string()),
    };
    let view = parse_create_view(
      &format!(
        "CREATE VIEW {view_name} AS SELECT id FROM {table_name}",
        view_name = view_name.escaped_string(),
        table_name = table_name.escaped_string()
      ),
      &tables,
    )
    .unwrap();
    let view_metadata = Arc::new(ViewMetadata::new(view, &[table.clone()]));

    let mut view_set = HashSet::<Arc<ViewMetadata>>::new();

    assert!(view_set.insert(view_metadata.clone()));
    assert_eq!(view_set.get(&view_name), Some(&view_metadata));
    assert_eq!(
      view_set.get(&QualifiedName::parse("view_name").unwrap()),
      Some(&view_metadata)
    );
  }
}
