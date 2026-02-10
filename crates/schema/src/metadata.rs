use jsonschema::Validator;
use lazy_static::lazy_static;
use log::*;
use regex::Regex;
use sqlite3_parser::ast::JoinType;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use trailbase_extension::jsonschema::JsonSchemaRegistry;

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
  #[error("Other: {0}")]
  Other(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum JsonColumnMetadata {
  SchemaName(String),
  Pattern(serde_json::Value),
}

impl JsonColumnMetadata {
  pub fn validate(
    &self,
    registry: &JsonSchemaRegistry,
    value: &serde_json::Value,
  ) -> Result<(), JsonSchemaError> {
    match self {
      Self::SchemaName(name) => {
        let Some(entry) = registry.get_schema(name) else {
          return Err(JsonSchemaError::NotFound(name.to_string()));
        };

        entry
          .validator
          .validate(value)
          .map_err(|_err| JsonSchemaError::Validation)?;
      }
      Self::Pattern(pattern) => {
        let schema =
          Validator::new(pattern).map_err(|err| JsonSchemaError::SchemaCompile(err.to_string()))?;

        if !schema.is_valid(value) {
          return Err(JsonSchemaError::Validation);
        }
      }
    }

    return Ok(());
  }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsonMetadata {
  pub columns: Vec<Option<JsonColumnMetadata>>,

  // Contains both, 'std.FileUpload' and 'std.FileUpload'.
  file_column_indexes: Vec<usize>,
}

impl JsonMetadata {
  fn from_columns(
    registry: &JsonSchemaRegistry,
    columns: &[Column],
  ) -> Result<Self, JsonSchemaError> {
    let columns = columns
      .iter()
      .map(|c| build_json_metadata(registry, c))
      .collect::<Result<Vec<_>, _>>()?;

    return Ok(Self {
      file_column_indexes: find_file_column_indexes(&columns),
      columns,
    });
  }

  pub fn has_file_columns(&self) -> bool {
    return !self.file_column_indexes.is_empty();
  }

  /// Contains both, 'std.FileUpload' and 'std.FileUpload'.
  pub fn file_column_indexes(&self) -> &[usize] {
    return &self.file_column_indexes;
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
  /// Metadata for CHECK(jsonschema()) columns.
  pub json_metadata: JsonMetadata,

  name_to_index: HashMap<String, usize>,
  // TODO: Add triggers once sqlparser supports a sqlite "CREATE TRIGGER" statements.
}

impl TableMetadata {
  /// Build a new TableMetadata instance containing TrailBase/RecordApi specific information.
  ///
  /// NOTE: The list of all tables is needed only to extract interger/UUIDv7 pk columns for foreign
  /// key relationships.
  pub fn new(
    registry: &JsonSchemaRegistry,
    table: Table,
    tables: &[Table],
  ) -> Result<Self, JsonSchemaError> {
    return Ok(TableMetadata {
      record_pk_column: find_record_pk_column_index_for_table(&table, tables),
      json_metadata: JsonMetadata::from_columns(registry, &table.columns)?,
      name_to_index: HashMap::<String, usize>::from_iter(
        table
          .columns
          .iter()
          .enumerate()
          .map(|(index, col)| (col.name.clone(), index)),
      ),
      schema: table,
    });
  }

  #[inline]
  pub fn name(&self) -> &QualifiedName {
    return &self.schema.name;
  }

  #[inline]
  pub fn column_index_by_name(&self, key: &str) -> Option<usize> {
    return self.name_to_index.get(key).copied();
  }

  pub fn column_by_name(&self, key: &str) -> Option<(usize, &Column)> {
    let index = self.column_index_by_name(key)?;
    return Some((index, &self.schema.columns[index]));
  }

  pub fn record_pk_column(&self) -> Option<(usize, &Column)> {
    let index = self.record_pk_column?;
    return self.schema.columns.get(index).map(|c| (index, c));
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
  pub json_metadata: Option<JsonMetadata>,
}

impl ViewMetadata {
  /// Build a new ViewMetadata instance containing TrailBase/RecordApi specific information.
  ///
  /// NOTE: The list of all tables is needed only to extract integer/UUID pk columns for foreign
  /// key relationships.
  pub fn new(
    registry: &JsonSchemaRegistry,
    view: View,
    tables: &[Table],
  ) -> Result<Self, JsonSchemaError> {
    let Some(column_mapping) = &view.column_mapping else {
      return Ok(ViewMetadata {
        name_to_index: HashMap::<String, usize>::default(),
        columns: None,
        record_pk_column: None,
        json_metadata: None,
        schema: view,
      });
    };

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

    return Ok(ViewMetadata {
      name_to_index,
      json_metadata: Some(JsonMetadata::from_columns(registry, &columns)?),
      columns: Some(columns),
      record_pk_column: find_record_pk_column_index_for_view(column_mapping, tables),
      schema: view,
    });
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

  pub fn column_by_name(&self, key: &str) -> Option<(usize, &Column)> {
    let index = self.column_index_by_name(key)?;
    let mapping = self.schema.column_mapping.as_ref()?;
    return Some((index, &mapping.columns[index].column));
  }

  pub fn record_pk_column(&self) -> Option<(usize, &Column)> {
    let index = self.record_pk_column?;
    return self.columns.as_ref()?.get(index).map(|c| (index, c));
  }
}

pub enum TableOrView<'a> {
  Table(&'a Table),
  View(&'a View),
}

impl<'a> TableOrView<'a> {
  pub fn qualified_name(&self) -> &'a QualifiedName {
    return match self {
      Self::Table(t) => &t.name,
      Self::View(v) => &v.name,
    };
  }

  pub fn columns(&self) -> Option<Vec<Column>> {
    return match self {
      Self::Table(t) => Some(t.columns.clone()),
      Self::View(v) => v
        .column_mapping
        .as_ref()
        .map(|m| m.columns.iter().map(|m| m.column.clone()).collect()),
    };
  }

  pub fn record_pk_column<T: Borrow<Table>>(&self, tables: &[T]) -> Option<(usize, &'a Column)> {
    return match self {
      Self::Table(t) => {
        find_record_pk_column_index_for_table(t, tables).map(|i| (i, &t.columns[i]))
      }
      Self::View(v) => {
        if let Some(ref mapping) = v.column_mapping {
          find_record_pk_column_index_for_view(mapping, tables)
            .map(|i| (i, &mapping.columns[i].column))
        } else {
          None
        }
      }
    };
  }
}

pub enum TableOrViewMetadata<'a> {
  Table(&'a TableMetadata),
  View(&'a ViewMetadata),
}

impl<'a> TableOrViewMetadata<'a> {
  pub fn qualified_name(&self) -> &'a QualifiedName {
    return match self {
      Self::Table(t) => &t.schema.name,
      Self::View(v) => &v.schema.name,
    };
  }

  pub fn columns(&self) -> Option<&'a [Column]> {
    return match self {
      Self::Table(t) => Some(&t.schema.columns),
      Self::View(v) => v.columns(),
    };
  }

  pub fn record_pk_column(&self) -> Option<(usize, &Column)> {
    return match self {
      Self::Table(t) => t.record_pk_column(),
      Self::View(v) => v.record_pk_column(),
    };
  }

  pub fn column_by_name(&self, name: &str) -> Option<(usize, &Column)> {
    return match self {
      Self::Table(t) => t.column_by_name(name),
      Self::View(v) => v.column_by_name(name),
    };
  }
}

/// Contains schema metadata for a bunch of TABLEs and VIEWs, which may belong to different
/// databases, e.g. all the TABLEs and VIEWs attached to a connection. Each can be uniquely
/// identified by their fully qualified name.
#[derive(Default)]
pub struct ConnectionMetadata {
  pub tables: HashMap<QualifiedName, TableMetadata>,
  pub views: HashMap<QualifiedName, ViewMetadata>,
}

impl ConnectionMetadata {
  pub fn from(tables: Vec<TableMetadata>, views: Vec<ViewMetadata>) -> Self {
    return Self {
      tables: tables.into_iter().map(|t| (t.name().clone(), t)).collect(),
      views: views.into_iter().map(|v| (v.name().clone(), v)).collect(),
    };
  }

  pub fn from_schemas(
    tables: Vec<Table>,
    views: Vec<View>,
    registry: &JsonSchemaRegistry,
  ) -> Result<Self, JsonSchemaError> {
    let table_metadata = tables
      .iter()
      .map(|t: &Table| TableMetadata::new(registry, t.clone(), &tables))
      .collect::<Result<Vec<TableMetadata>, _>>()?;

    let view_metadata = views
      .into_iter()
      .map(|view: View| ViewMetadata::new(registry, view, &tables))
      .collect::<Result<Vec<ViewMetadata>, _>>()?;

    return Ok(ConnectionMetadata::from(table_metadata, view_metadata));
  }

  pub fn get_table(&self, name: &QualifiedName) -> Option<&TableMetadata> {
    return self.tables.get(name);
  }

  pub fn get_view(&self, name: &QualifiedName) -> Option<&ViewMetadata> {
    return self.views.get(name);
  }

  pub fn get_table_or_view(&self, name: &QualifiedName) -> Option<TableOrViewMetadata<'_>> {
    if let Some(table) = self.tables.get(name) {
      return Some(TableOrViewMetadata::Table(table));
    }
    if let Some(view) = self.views.get(name) {
      return Some(TableOrViewMetadata::View(view));
    }
    return None;
  }

  pub fn tables(&self) -> Vec<&Table> {
    return self.tables.values().map(|t| &t.schema).collect();
  }

  pub fn views(&self) -> Vec<&View> {
    return self.views.values().map(|v| &v.schema).collect();
  }
}

fn build_json_metadata(
  registry: &JsonSchemaRegistry,
  col: &Column,
) -> Result<Option<JsonColumnMetadata>, JsonSchemaError> {
  for opt in &col.options {
    if let Some(metadata) = extract_json_metadata(registry, opt)? {
      return Ok(Some(metadata));
    }
  }

  return Ok(None);
}

pub(crate) fn extract_json_metadata(
  registry: &JsonSchemaRegistry,
  opt: &ColumnOption,
) -> Result<Option<JsonColumnMetadata>, JsonSchemaError> {
  let ColumnOption::Check(check) = opt else {
    return Ok(None);
  };

  lazy_static! {
    static ref SCHEMA_RE: Regex =
      Regex::new(r#"(?smR)jsonschema\s*\(\s*[\['"](?<name>.*)[\]'"]\s*,.+?\)"#)
        .expect("infallible");
  }

  if let Some(cap) = SCHEMA_RE.captures(check) {
    let name = &cap["name"];

    let Some(_schema) = registry.get_schema(name) else {
      return Err(JsonSchemaError::NotFound(format!(
        "Json schema {name} not found"
      )));
    };

    return Ok(Some(JsonColumnMetadata::SchemaName(name.to_string())));
  }

  lazy_static! {
    static ref MATCHES_RE: Regex =
      Regex::new(r"(?smR)jsonschema_matches\s*\(.+?(?<pattern>\{.*\}).+?\)").expect("infallible");
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
        && foreign_table == user_table_name
        && referred_columns.len() == 1
        && referred_columns[0] == "id"
      {
        indexes.push(index);
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

fn is_suitable_record_pk_column<T: Borrow<Table>>(column: &Column, tables: &[T]) -> bool {
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
            let Some(referred_table) = tables
              .iter()
              .find(|t| (*t).borrow().name.name == *foreign_table)
            else {
              warn!("Failed to get foreign key schema for {foreign_table}");
              return false;
            };

            let Some(foreign_column) = referred_table
              .borrow()
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
fn find_record_pk_column_index_for_table<T: Borrow<Table>>(
  table: &Table,
  tables: &[T],
) -> Option<usize> {
  if table.strict {
    for (index, column) in table.columns.iter().enumerate() {
      if is_suitable_record_pk_column(column, tables) {
        return Some(index);
      }
    }
  }
  return None;
}

pub(crate) fn find_record_pk_column_index_for_view<T: Borrow<Table>>(
  column_mapping: &ColumnMapping,
  tables: &[T],
) -> Option<usize> {
  let grouped_on_non_pk = if let Some(group_by_index) = column_mapping.group_by {
    // If we're grouping on a PK things are trivial, this will yield groups of size one :).
    let column = &column_mapping.columns[group_by_index];
    if is_suitable_record_pk_column(&column.column, tables) {
      info!(
        "Using GROUP BY on the unique column '{}' is a no-op.",
        column.column.name
      );
      return Some(group_by_index);
    } else {
      // Now things are difficult. Because PK may be aggregated, e.g.:
      //   `SELECT AVG(a.pk) FOR a GROUP BY a.other`
      // destroys PK properties.
      true
    }
  } else {
    false
  };

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
      // No grouping. Just return the suitable PK.
      if !grouped_on_non_pk {
        return Some(index);
      }

      // Otherwise we have to make sure that the candidate is aggregated in a way that
      // preserves its uniqueness.
      // To be clear, using aggregated PKs is a BAD idea. For example,
      //   SELECT MAX(a.pk) AS id, COUNT(*) AS size FROM a GROUP BY a.other
      // yields a unique but unstable `id`. Adding a new record to `a` may invalidate
      // externally kept `id`s.
      //
      // QUESTION: Should we even support this? Folks wanted this in the context of
      // `listing` VIEWs, i.e. you don't want to access individual records but merely
      // get a list of counts.
      if let Some(agg) = &mapped_column.aggregation
        && builtin_pk_preserving_type(agg)
      {
        warn!(
          "Found aggregate PK '{agg}({}). Support is experimental. This yields unstable record IDs unsuitable for individual record access.",
          mapped_column.column.name
        );
        return Some(index);
      }
    }
  }
  return None;
}

fn builtin_pk_preserving_type(name: &str) -> bool {
  let name = name.to_uppercase();
  return matches!(name.as_str(), "MAX" | "MIN");
}

#[cfg(test)]
mod tests {
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
    let registry = JsonSchemaRegistry::from_schemas(vec![]);
    let metadata = TableMetadata::new(&registry, table, &tables).unwrap();

    assert_eq!("table0", metadata.name().name);
    assert_eq!("col1", metadata.schema.columns[2].name);
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

      let view_metadata = ViewMetadata::new(&registry, table_view, &tables).unwrap();

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

      let view_metadata = ViewMetadata::new(&registry, table_view, &tables).unwrap();

      let uuidv7_col = view_metadata.record_pk_column().unwrap();
      let columns = view_metadata.columns().unwrap();
      assert_eq!(columns.len(), 3);
      assert_eq!(columns[uuidv7_col.0].name, "id");
    }
  }

  #[test]
  fn test_parse_create_view_with_subquery() {
    let registry = JsonSchemaRegistry::from_schemas(vec![]);

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

      let metadata = ViewMetadata::new(&registry, view, &tables).unwrap();
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

      let metadata = ViewMetadata::new(&registry, view, &tables).unwrap();
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

      let metadata = ViewMetadata::new(&registry, view, &tables).unwrap();
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
    let registry = JsonSchemaRegistry::from_schemas(vec![]);

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

      let metadata = ViewMetadata::new(&registry, view, &tables).unwrap();
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

        let metadata = ViewMetadata::new(&registry, view, &tables).unwrap();
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
    let registry = JsonSchemaRegistry::from_schemas(vec![]);

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

          let metadata = ViewMetadata::new(&registry, view, &tables).unwrap();
          assert_eq!(Some(1), metadata.record_pk_column().map(|c| c.0));
        }
      }

      {
        let view = parse_create_view(
          "CREATE VIEW v AS SELECT a.data, a.id FROM a RIGHT JOIN b ON a.id = b.id GROUP BY a.id;",
          &tables,
        )
        .unwrap();

        let metadata = ViewMetadata::new(&registry, view, &tables).unwrap();
        assert_eq!(Some(1), metadata.record_pk_column().map(|c| c.0));
      }
    }
  }

  #[test]
  fn test_parse_create_view_from_issue_99() {
    let registry = JsonSchemaRegistry::from_schemas(vec![]);

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

      let metadata = ViewMetadata::new(&registry, view, &tables).unwrap();
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

      let metadata = ViewMetadata::new(&registry, view, &tables).unwrap();
      assert_eq!(Some(0), metadata.record_pk_column().map(|c| c.0));
    }
  }
}
