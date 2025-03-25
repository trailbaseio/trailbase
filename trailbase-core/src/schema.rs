use log::*;
use serde::{Deserialize, Serialize};
use sqlite3_parser::ast::{
  fmt::ToTokens, ColumnDefinition, CreateTableBody, Expr, FromClause, Name, SelectTable, Stmt,
  TableOptions,
};
use std::collections::HashMap;
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum SchemaError {
  #[error("Missing ObjectName")]
  MissingName,
  #[error("Precondition failed: {0}")]
  Precondition(Box<dyn std::error::Error + Send + Sync>),
}

// This file contains table schema and index representations. Originally, they were mostly
// adaptations of sqlparser's CreateX AST representations (we've since moved to sqlite3_parser).
// This serves two purposes:
//
//  * We'd like some representation that we can construct on the client with type-safety. We could
//    also consider using proto here, but ts-rs let's us "skip" some fields.
//  * But also, there's a fundamental difference between an AST that represents a specific SQL
//    program and a more abstract semantic representation of the schema, e.g. we don't care in which
//    order indexes were constructed or what quotes were used...
//
// NOTE: We're very much "over-wrapping" here entering the space of the exact-program AST domain.
// This is mostly convenient for testing our code by transforming back and forth and checking the
// output is stable. We can use "skip" to remove some more "representational" details from the API.
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct ForeignKey {
  pub name: Option<String>,
  pub columns: Vec<String>,
  pub foreign_table: String,
  pub referred_columns: Vec<String>,
  pub on_delete: Option<ReferentialAction>,
  pub on_update: Option<ReferentialAction>,
}

impl ForeignKey {
  fn to_fragment(&self) -> String {
    return format!(
      "{name} FOREIGN KEY ({cols}) REFERENCES {foreign_table} ({ref_cols}) {on_delete} {on_update}",
      name = self
        .name
        .as_ref()
        .map_or_else(|| "".to_string(), |n| format!("CONSTRAINT {n}")),
      cols = self.columns.join(", "),
      foreign_table = self.foreign_table,
      ref_cols = self.referred_columns.join(", "),
      on_delete = self.on_delete.as_ref().map_or_else(
        || "".to_string(),
        |action| format!("ON DELETE {}", action.to_fragment())
      ),
      on_update = self.on_update.as_ref().map_or_else(
        || "".to_string(),
        |action| format!("ON UPDATE {}", action.to_fragment())
      ),
    );
  }
}

// TODO: Our table constraints are generally very incomplete:
// https://www.sqlite.org/syntax/table-constraint.html.
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct UniqueConstraint {
  pub name: Option<String>,
  /// Identifiers of the columns that are unique.
  /// TODO: Should be indexed/ordered column.
  pub columns: Vec<String>,
}

impl UniqueConstraint {
  fn to_fragment(&self) -> String {
    return format!(
      "{name}UNIQUE ({cols}) {conflict_clause}",
      name = self
        .name
        .as_ref()
        .map_or_else(|| "".to_string(), |n| format!("CONSTRAINT {n} ")),
      cols = self.columns.join(", "),
      conflict_clause = "",
    );
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct ColumnOrder {
  pub column_name: String,
  pub ascending: Option<bool>,
  pub nulls_first: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub enum ReferentialAction {
  Restrict,
  Cascade,
  SetNull,
  NoAction,
  SetDefault,
}

impl From<sqlite3_parser::ast::RefAct> for ReferentialAction {
  fn from(action: sqlite3_parser::ast::RefAct) -> Self {
    use sqlite3_parser::ast::RefAct;
    match action {
      RefAct::Restrict => ReferentialAction::Restrict,
      RefAct::Cascade => ReferentialAction::Cascade,
      RefAct::SetNull => ReferentialAction::SetNull,
      RefAct::NoAction => ReferentialAction::NoAction,
      RefAct::SetDefault => ReferentialAction::SetDefault,
    }
  }
}

impl ReferentialAction {
  // https://www.sqlite.org/syntax/foreign-key-clause.html
  fn to_fragment(&self) -> &'static str {
    return match self {
      Self::Restrict => "RESTRICT",
      Self::Cascade => "CASCADE",
      Self::SetNull => "SET NULL",
      Self::NoAction => "NO ACTION",
      Self::SetDefault => "SET DEFAULT",
    };
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub enum GeneratedExpressionMode {
  Virtual,
  Stored,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub enum ColumnOption {
  Null,
  NotNull,
  Default(String),
  // NOTE: Unique { is_primary: true} means PrimaryKey.
  Unique {
    is_primary: bool,
  },
  ForeignKey {
    foreign_table: String,
    referred_columns: Vec<String>,
    on_delete: Option<ReferentialAction>,
    on_update: Option<ReferentialAction>,
  },
  Check(String),
  OnUpdate(String),
  Generated {
    expr: String,
    mode: Option<GeneratedExpressionMode>,
  },
}

impl ColumnOption {
  fn to_fragment(&self) -> String {
    return match self {
      Self::Null => "NULL".to_string(),
      Self::NotNull => "NOT NULL".to_string(),
      Self::Default(v) => format!("DEFAULT {v}"),
      Self::Unique { is_primary } => {
        if *is_primary {
          "PRIMARY KEY".to_string()
        } else {
          "UNIQUE".to_string()
        }
      }
      Self::ForeignKey {
        foreign_table,
        referred_columns,
        on_delete,
        on_update,
      } => {
        format!(
          "REFERENCES {foreign_table}{ref_col} {on_delete} {on_update}",
          ref_col = match referred_columns.len() {
            0 => "".to_string(),
            _ => format!("({})", referred_columns.join(",")),
          },
          on_delete = on_delete.as_ref().map_or_else(
            || "".to_string(),
            |action| format!("ON DELETE {}", action.to_fragment())
          ),
          on_update = on_update.as_ref().map_or_else(
            || "".to_string(),
            |action| format!("ON UPDATE {}", action.to_fragment())
          ),
        )
      }
      Self::Check(expr) => format!("CHECK({expr})"),
      Self::OnUpdate(expr) => expr.to_string(),
      Self::Generated { expr, mode } => format!(
        "GENERATED ALWAYS AS ({expr}) {m}",
        m = match mode {
          Some(GeneratedExpressionMode::Stored) => "STORED",
          Some(GeneratedExpressionMode::Virtual) => "VIRTUAL",
          None => "",
        }
      ),
    };
  }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS, PartialEq)]
pub enum ColumnDataType {
  Null,

  // Strict column/storage types.
  Any,
  Blob,
  Text,
  Integer,
  Real,
  Numeric, // not allowed in strict mode.

  // Other higher-level or affine types.
  #[allow(clippy::upper_case_acronyms)]
  JSON,
  #[allow(clippy::upper_case_acronyms)]
  JSONB,

  // See 3.1.1. https://www.sqlite.org/datatype3.html.
  //
  // Types with INTEGER affinity.
  Int,
  TinyInt,
  SmallInt,
  MediumInt,
  BigInt,
  UnignedBigInt,
  Int2,
  Int4,
  Int8,

  // Types with TEXT affinity.
  Character,
  Varchar,
  VaryingCharacter,
  NChar,
  NativeCharacter,
  NVarChar,
  Clob,

  // Types with REAL affinity.
  Double,
  DoublePrecision,
  Float,

  // Types with NUMERIC affinity.
  Boolean,
  Decimal,
  Date,
  DateTime,
}

impl ColumnDataType {
  fn from_type_name(type_name: &str) -> Option<Self> {
    return Some(match type_name.to_uppercase().as_str() {
      "UNSPECIFIED" => ColumnDataType::Null,
      "ANY" => ColumnDataType::Any,
      "BLOB" => ColumnDataType::Blob,
      "TEXT" => ColumnDataType::Text,
      "INTEGER" => ColumnDataType::Integer,
      "REAL" => ColumnDataType::Real,
      "NUMERIC" => ColumnDataType::Numeric,

      // JSON types,
      "JSON" => ColumnDataType::JSON,
      "JSONB" => ColumnDataType::JSONB,

      // See 3.1.1. https://www.sqlite.org/datatype3.html.
      //
      // Types with INTEGER affinity.
      "INT" => ColumnDataType::Int,
      "TINYINT" => ColumnDataType::TinyInt,
      "SMALLINT" => ColumnDataType::SmallInt,
      "MEDIUMINT" => ColumnDataType::MediumInt,
      "BIGINT" => ColumnDataType::BigInt,
      "UNSIGNED BIG INT" => ColumnDataType::UnignedBigInt,
      "INT2" => ColumnDataType::Int2,
      "INT4" => ColumnDataType::Int4,
      "INT8" => ColumnDataType::Int8,

      // Types with TEXT affinity.
      "CHARACTER" => ColumnDataType::Character,
      "VARCHAR" => ColumnDataType::Varchar,
      "VARYING CHARACTER" => ColumnDataType::VaryingCharacter,
      "NCHAR" => ColumnDataType::NChar,
      "NATIVE CHARACTER" => ColumnDataType::NativeCharacter,
      "NVARCHAR" => ColumnDataType::NVarChar,
      "CLOB" => ColumnDataType::Clob,

      // Types with REAL affinity.
      "DOUBLE" => ColumnDataType::Double,
      "DOUBLE PRECISION" => ColumnDataType::DoublePrecision,
      "FLOAT" => ColumnDataType::Float,

      // Types with NUMERIC affinity.
      "BOOLEAN" => ColumnDataType::Boolean,
      "DECIMAL" => ColumnDataType::Decimal,
      "DATE" => ColumnDataType::Date,
      "DATETIME" => ColumnDataType::DateTime,

      _x => {
        debug!("Unexpected data type: {_x:?}");
        return None;
      }
    });
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct Column {
  pub name: String,
  pub data_type: ColumnDataType,
  pub options: Vec<ColumnOption>,
}

impl Column {
  fn to_fragment(&self) -> String {
    let options: Vec<String> = self.options.iter().map(|o| o.to_fragment()).collect();

    return if options.is_empty() {
      format!(
        "'{name}' {data_type}",
        name = self.name,
        data_type = format!("{:?}", self.data_type).to_uppercase(),
      )
    } else {
      format!(
        "'{name}' {data_type} {options}",
        name = self.name,
        data_type = format!("{:?}", self.data_type).to_uppercase(),
        options = options.join(" "),
      )
    };
  }
}

impl Column {
  pub fn is_primary(&self) -> bool {
    self
      .options
      .iter()
      .any(|opt| matches!(opt, ColumnOption::Unique { is_primary } if *is_primary ))
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Table {
  pub name: String,
  pub strict: bool,

  // Column definition and column-level constraints.
  pub columns: Vec<Column>,

  // Table-level constraints, e.g. composite uniqueness or foreign keys. Columns may have their own
  // column-level constraints a.k.a. Column::options.
  pub foreign_keys: Vec<ForeignKey>,
  pub unique: Vec<UniqueConstraint>,

  // NOTE: consider parsing "CREATE VIRTUAL TABLE" into a separate struct.
  pub virtual_table: bool,
  pub temporary: bool,
}

impl Table {
  pub(crate) fn create_table_statement(&self) -> String {
    if self.virtual_table {
      // https://www.sqlite.org/lang_createvtab.html
      panic!("Not implemented");
    }

    let mut column_defs_and_table_constraints: Vec<String> = vec![];

    let column_defs = self
      .columns
      .iter()
      .map(|c| c.to_fragment())
      .collect::<Vec<_>>();
    column_defs_and_table_constraints.extend(column_defs);

    // Example: UNIQUE (email),
    let unique_table_constraints = self
      .unique
      .iter()
      .map(|unique| unique.to_fragment())
      .collect::<Vec<_>>();
    column_defs_and_table_constraints.extend(unique_table_constraints);

    // Example: FOREIGN KEY(user_id) REFERENCES table(id) ON DELETE CASCADE
    let fk_table_constraints = self
      .foreign_keys
      .iter()
      .map(|fk| fk.to_fragment())
      .collect::<Vec<_>>();
    column_defs_and_table_constraints.extend(fk_table_constraints);

    return format!(
      "CREATE{temporary} TABLE '{name}' ({col_defs_and_constraints}){strict}",
      temporary = if self.temporary { " TEMPORARY" } else { "" },
      name = self.name,
      col_defs_and_constraints = column_defs_and_table_constraints.join(", "),
      strict = if self.strict { " STRICT" } else { "" },
    );
  }
}

#[derive(Clone, Default, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct TableIndex {
  pub name: String,
  pub table_name: String,
  pub columns: Vec<ColumnOrder>,
  pub unique: bool,
  pub predicate: Option<String>,

  #[ts(skip)]
  #[serde(default)]
  pub if_not_exists: bool,
}

impl TableIndex {
  pub(crate) fn create_index_statement(&self) -> String {
    let indexed_columns_vec: Vec<String> = self
      .columns
      .iter()
      .map(|c| {
        format!(
          "{name} {order}",
          name = c.column_name,
          order = c
            .ascending
            .map_or("", |asc| if asc { "ASC" } else { "DESC" })
        )
      })
      .collect();

    return format!(
      "CREATE {unique} INDEX {if_not_exists} '{name}' ON '{table_name}' ({indexed_columns}) {predicate}",
      unique = if self.unique { "UNIQUE" } else { "" },
      if_not_exists = if self.if_not_exists {
        "IF NOT EXISTS"
      } else {
        ""
      },
      name = self.name,
      table_name = self.table_name,
      indexed_columns = indexed_columns_vec.join(", "),
      predicate = self
        .predicate
        .as_ref()
        .map_or_else(|| "".to_string(), |p| format!("WHERE {p}")),
    );
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct View {
  pub name: String,

  /// Columns may be inferred from a view's query.
  ///
  /// Views can be defined with arbitrary queries referencing arbitrary sources: tables, views,
  /// functions, ..., which makes them inherently not type safe and therefore their columns not
  /// well defined.
  pub columns: Option<Vec<Column>>,

  pub query: String,

  pub temporary: bool,

  #[ts(skip)]
  pub if_not_exists: bool,
}

impl TryFrom<sqlite3_parser::ast::Stmt> for Table {
  type Error = SchemaError;

  fn try_from(value: sqlite3_parser::ast::Stmt) -> Result<Self, Self::Error> {
    return match value {
      Stmt::CreateTable {
        temporary,
        tbl_name,
        body,
        ..
      } => {
        let CreateTableBody::ColumnsAndConstraints {
          columns,
          constraints,
          options,
        } = body
        else {
          return Err(SchemaError::Precondition(
            "expected cols and constraints, got AsSelect".into(),
          ));
        };

        let (foreign_keys, unique) = match &constraints {
          None => (vec![], vec![]),
          Some(constraints) => {
            use sqlite3_parser::ast::TableConstraint;

            let foreign_keys: Vec<ForeignKey> = constraints
              .iter()
              .filter_map(|constraint| match &constraint.constraint {
                TableConstraint::ForeignKey {
                  columns,
                  clause,
                  deref_clause: _,
                } => {
                  let mut on_delete: Option<ReferentialAction> = None;
                  let mut on_update: Option<ReferentialAction> = None;
                  for arg in &clause.args {
                    use sqlite3_parser::ast::RefArg;

                    match arg {
                      RefArg::OnDelete(action) => {
                        on_delete = Some((*action).into());
                      }
                      RefArg::OnUpdate(action) => {
                        on_update = Some((*action).into());
                      }
                      _ => {}
                    }
                  }

                  Some(ForeignKey {
                    name: constraint.name.as_ref().map(|name| name.to_string()),
                    foreign_table: unquote(clause.tbl_name.clone()),
                    columns: columns.iter().map(|c| c.col_name.to_string()).collect(),
                    referred_columns: clause.columns.as_ref().map_or_else(Vec::new, |columns| {
                      columns.iter().map(|c| c.col_name.to_string()).collect()
                    }),
                    on_update,
                    on_delete,
                  })
                }
                _ => None,
              })
              .collect();

            let unique: Vec<UniqueConstraint> = constraints
              .iter()
              .filter_map(|constraint| match &constraint.constraint {
                TableConstraint::Unique {
                  columns,
                  conflict_clause: _,
                } => Some(UniqueConstraint {
                  name: constraint.name.as_ref().map(|name| name.to_string()),
                  columns: columns.iter().map(|c| c.expr.to_string()).collect(),
                }),
                _ => None,
              })
              .collect();

            (foreign_keys, unique)
          }
        };

        let columns: Vec<_> = columns
          .into_iter()
          .map(|(name, def): (Name, ColumnDefinition)| {
            let ColumnDefinition {
              col_name,
              col_type,
              constraints,
            } = def;
            assert_eq!(name, col_name);

            let name = unquote(col_name);
            assert!(!name.is_empty());

            let data_type: ColumnDataType = match col_type {
              Some(x) => x.into(),
              None => ColumnDataType::Null,
            };

            let options: Vec<ColumnOption> = constraints
              .into_iter()
              .map(|named_constraint| named_constraint.constraint.into())
              .collect();

            return Column {
              name,
              data_type,
              options,
            };
          })
          .collect();

        // WARN: SQLite escaping is weird, altering a table adds double quote escaping and
        // sqlite3_parser unlike sqlparser, doesn't parse out the escaping.
        //
        // sqlite> CREATE TABLE foo (x text);
        // sqlite> SELECT sql FROM main.sqlite_schema;
        //   CREATE TABLE foo (x text)
        // sqlite> ALTER TABLE foo RENAME TO bar
        // sqlite> SELECT sql FROM main.sqlite_schema;
        //   CREATE TABLE "bar" (x text)
        //
        // TODO: factor out QualifiedNamed conversion.
        let table_name = unquote(tbl_name.name);

        Ok(Table {
          name: table_name,
          strict: options.contains(TableOptions::STRICT),
          columns,
          foreign_keys,
          unique,
          virtual_table: false,
          temporary,
        })
      }
      Stmt::CreateVirtualTable {
        tbl_name,
        args: _args,
        ..
      } => Ok(Table {
        name: unquote(tbl_name.name),
        strict: false,
        columns: vec![],
        foreign_keys: vec![],
        unique: vec![],
        virtual_table: true,
        temporary: false,
      }),
      _ => Err(SchemaError::Precondition(
        format!("expected 'CREATE TABLE', got: {value:?}").into(),
      )),
    };
  }
}

impl From<sqlite3_parser::ast::Type> for ColumnDataType {
  fn from(data_type: sqlite3_parser::ast::Type) -> Self {
    return ColumnDataType::from_type_name(&data_type.name).unwrap_or(ColumnDataType::Null);
  }
}

impl From<sqlite3_parser::ast::ColumnConstraint> for ColumnOption {
  fn from(constraint: sqlite3_parser::ast::ColumnConstraint) -> Self {
    type Constraint = sqlite3_parser::ast::ColumnConstraint;

    return match constraint {
      Constraint::PrimaryKey {
        conflict_clause: _, ..
      } => ColumnOption::Unique { is_primary: true },
      Constraint::Unique(_) => ColumnOption::Unique { is_primary: false },
      Constraint::Check(expr) => ColumnOption::Check(expr.to_string()),
      Constraint::ForeignKey { clause, .. } => {
        let columns = clause.columns.unwrap_or(vec![]);

        ColumnOption::ForeignKey {
          foreign_table: clause.tbl_name.to_string(),
          referred_columns: columns
            .into_iter()
            .map(|c| c.col_name.to_string())
            .collect(),
          on_delete: None,
          on_update: None,
        }
      }
      Constraint::NotNull { .. } => ColumnOption::NotNull,
      Constraint::Default(expr) => ColumnOption::Default(expr.to_string()),
      Constraint::Generated { expr, typ } => ColumnOption::Generated {
        expr: expr.to_string(),
        mode: typ.and_then(|t| match t.0.as_str() {
          "VIRTUAL" => Some(GeneratedExpressionMode::Virtual),
          "STORED" => Some(GeneratedExpressionMode::Stored),
          x => {
            warn!("Unexpected generated column mode: {x}");
            None
          }
        }),
      },
      Constraint::Collate { .. } | Constraint::Defer(_) => {
        panic!("Not implemented: {constraint:?}");
      }
    };
  }
}

impl TryFrom<sqlite3_parser::ast::Stmt> for TableIndex {
  type Error = SchemaError;

  fn try_from(value: sqlite3_parser::ast::Stmt) -> Result<Self, Self::Error> {
    return match value {
      sqlite3_parser::ast::Stmt::CreateIndex {
        unique,
        if_not_exists,
        idx_name,
        tbl_name,
        columns,
        where_clause,
      } => Ok(TableIndex {
        name: unquote(idx_name.name),
        table_name: unquote(tbl_name),
        columns: columns
          .into_iter()
          .map(|order_expr| ColumnOrder {
            column_name: unquote_expr(order_expr.expr),
            ascending: order_expr
              .order
              .map(|order| order == sqlite3_parser::ast::SortOrder::Asc),
            nulls_first: order_expr
              .nulls
              .map(|order| order == sqlite3_parser::ast::NullsOrder::First),
          })
          .collect(),
        unique,
        predicate: where_clause.map(|clause| clause.to_string()),
        if_not_exists,
      }),
      _ => Err(SchemaError::Precondition(
        format!("expected 'CREATE INDEX', got: {value:?}").into(),
      )),
    };
  }
}

struct SelectFormatter(sqlite3_parser::ast::Select);

impl std::fmt::Display for SelectFormatter {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    self.0.to_fmt(f)
  }
}

impl View {
  pub fn from(value: sqlite3_parser::ast::Stmt, tables: &[Table]) -> Result<Self, SchemaError> {
    return match value {
      sqlite3_parser::ast::Stmt::CreateView {
        temporary,
        if_not_exists,
        view_name,
        columns,
        select,
      } => {
        let columns = match columns.is_some() {
          true => {
            info!("CREATE VIEW column filtering not supported (yet)");
            None
          }
          false => try_extract_column_mapping((*select).clone(), tables)?.map(|column_mapping| {
            column_mapping
              .into_iter()
              .map(|mapping| mapping.column)
              .collect()
          }),
        };

        Ok(View {
          name: view_name.to_string(),
          columns,
          query: SelectFormatter(*select).to_string(),
          temporary,
          if_not_exists,
        })
      }
      _ => Err(SchemaError::Precondition(
        format!("expected 'CREATE VIEW', got: {value:?}").into(),
      )),
    };
  }
}

fn to_entry(
  qn: sqlite3_parser::ast::QualifiedName,
  alias: Option<sqlite3_parser::ast::As>,
) -> (String, String) {
  return (
    alias
      .and_then(|alias| {
        if let sqlite3_parser::ast::As::As(name) = alias {
          return Some(name.to_string());
        }
        None
      })
      .unwrap_or_else(|| qn.to_string()),
    qn.to_string(),
  );
}

#[derive(Clone, Debug)]
#[allow(unused)]
struct ReferredColumn {
  table_name: String,
  column_name: String,
}

#[derive(Clone, Debug)]
struct ColumnMapping {
  column: Column,

  #[allow(unused)]
  referred_column: Option<ReferredColumn>,
}

fn try_extract_column_mapping(
  select: sqlite3_parser::ast::Select,
  tables: &[Table],
) -> Result<Option<Vec<ColumnMapping>>, SchemaError> {
  let body = select.body;

  if body.compounds.is_some() {
    return Ok(None);
  }

  let sqlite3_parser::ast::OneSelect::Select {
    distinctness,
    columns,
    from,
    where_clause: _,
    group_by,
    window_clause,
  } = body.select
  else {
    return Ok(None);
  };

  if distinctness.is_some() || group_by.is_some() || window_clause.is_some() {
    return Ok(None);
  }

  // First build list of referenced tables and their aliases.
  let Some(FromClause { select, joins, .. }) = from else {
    return Ok(None);
  };
  let Some(select) = select else {
    return Ok(None);
  };
  let SelectTable::Table(fqn, alias, _indexed) = *select else {
    return Ok(None);
  };

  // Use IndexMap to preserve insertion order.
  let mut table_names = indexmap::IndexMap::<String, String>::from([to_entry(fqn, alias)]);

  if let Some(joins) = joins {
    for join in joins {
      let SelectTable::Table(fqn, alias, _indexed) = join.table else {
        return Ok(None);
      };

      let entry = to_entry(fqn, alias);
      table_names.insert(entry.0, entry.1);
    }
  }

  // Now we should have a map of all involved tables and their aliases (if any).
  let all_tables: HashMap<String, &Table> = tables.iter().map(|t| (t.name.clone(), t)).collect();
  let mut all_columns = HashMap::<String, (&Table, &Column)>::new();

  // Make sure we know all tables and all tables are strict.
  for table_name in table_names.values() {
    match all_tables.get(table_name) {
      Some(table) => {
        if !table.strict {
          info!("Skipping view: referenced table: {table_name} not strict");
          return Ok(None);
        }

        for col in &table.columns {
          all_columns.insert(col.name.clone(), (table, col));
        }
      }
      None => {
        return Err(SchemaError::Precondition(
          format!("View's SELECT references missing table: {table_name}").into(),
        ));
      }
    };
  }

  let mut mapping: Vec<ColumnMapping> = vec![];
  for col in columns {
    use sqlite3_parser::ast::Expr;
    use sqlite3_parser::ast::ResultColumn;

    match col {
      ResultColumn::Star => {
        for table_name in table_names.values() {
          let table = all_tables.get(table_name).expect("checked above");
          for c in &table.columns {
            mapping.push(ColumnMapping {
              column: c.clone(),
              referred_column: Some(ReferredColumn {
                table_name: table.name.clone(),
                column_name: c.name.clone(),
              }),
            });
          }
        }
      }
      ResultColumn::TableStar(name) => {
        let name = name.to_string();
        let Some(table_name) = table_names.get(&name) else {
          return Err(SchemaError::Precondition(
            format!("Missing alias: {name}").into(),
          ));
        };

        let table = all_tables.get(table_name).expect("checked above");
        for c in &table.columns {
          mapping.push(ColumnMapping {
            column: c.clone(),
            referred_column: Some(ReferredColumn {
              table_name: table.name.clone(),
              column_name: c.name.clone(),
            }),
          });
        }
      }
      ResultColumn::Expr(expr, alias) => match expr {
        Expr::Id(id) => {
          let col_name = &id.0;
          let Some((table, column)) = all_columns.get(col_name) else {
            return Err(SchemaError::Precondition(
              format!("Missing columns: {id:?}").into(),
            ));
          };

          let name = alias
            .and_then(|alias| {
              if let sqlite3_parser::ast::As::As(name) = alias {
                return Some(name.to_string());
              }
              None
            })
            .unwrap_or_else(|| column.name.clone());

          mapping.push(ColumnMapping {
            column: Column {
              name,
              data_type: column.data_type,
              options: column.options.clone(),
            },
            referred_column: Some(ReferredColumn {
              table_name: table.name.clone(),
              column_name: column.name.clone(),
            }),
          });
        }
        Expr::Qualified(qualifier, name) => {
          let qualifier = qualifier.to_string();
          let col_name = name.to_string();

          let Some(table_name) = table_names.get(&qualifier) else {
            return Err(SchemaError::Precondition(
              format!("Missing table with qualifier: {qualifier}").into(),
            ));
          };

          let table = all_tables.get(table_name).expect("checked above");
          let Some(column) = table.columns.iter().find(|c| c.name == col_name) else {
            return Err(SchemaError::Precondition(
              format!("Missing col: {col_name}").into(),
            ));
          };

          let name = alias
            .and_then(|alias| {
              if let sqlite3_parser::ast::As::As(name) = alias {
                return Some(name.to_string());
              }
              None
            })
            .unwrap_or_else(|| column.name.clone());

          mapping.push(ColumnMapping {
            column: Column {
              name,
              data_type: column.data_type,
              options: column.options.clone(),
            },
            referred_column: Some(ReferredColumn {
              table_name: table.name.clone(),
              column_name: column.name.clone(),
            }),
          });
        }
        Expr::Cast { expr: _, type_name } => {
          let Some(type_name) = type_name else {
            return Err(SchemaError::Precondition(
              "Missing type_name in cast".into(),
            ));
          };
          let Some(data_type) = ColumnDataType::from_type_name(&type_name.name) else {
            return Err(SchemaError::Precondition(
              "Missing type_name in cast".into(),
            ));
          };

          let Some(name) = alias.and_then(|alias| {
            if let sqlite3_parser::ast::As::As(name) = alias {
              return Some(name.to_string());
            }
            None
          }) else {
            return Err(SchemaError::Precondition("Missing alias in cast".into()));
          };

          mapping.push(ColumnMapping {
            column: Column {
              name,
              data_type,
              options: vec![ColumnOption::Null],
            },
            referred_column: None,
          });
        }
        _x => {
          // We cannot map arbitrary expressions.
          #[cfg(debug_assertions)]
          debug!("skipping expr: {_x:?}");

          return Ok(None);
        }
      },
    };
  }

  return Ok(Some(mapping));
}

fn unquote(name: Name) -> String {
  let n = name.0.as_bytes();
  if n.is_empty() {
    return name.0;
  }

  return match n[0] {
    b'"' | b'`' | b'\'' | b'[' => {
      assert!(n.len() > 2);
      name.0[1..n.len() - 1].to_string()
    }
    _ => name.0,
  };
}

fn unquote_expr(expr: Expr) -> String {
  return match expr {
    Expr::Name(n) => unquote(n),
    x => x.to_string(),
  };
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::constants::USER_TABLE;
  use crate::table_metadata::sqlite3_parse_into_statement;

  #[test]
  fn test_unquote() {
    assert_eq!(unquote(Name("".to_string())), "");
    assert_eq!(unquote(Name("['``']".to_string())), "'``'");
    assert_eq!(unquote(Name("\"[]\"".to_string())), "[]");
  }

  #[test]
  fn test_create_table_statement_quoting() {
    let statement = format!(
      r#"
      CREATE TABLE "table" (
          'index'       TEXT,
          `delete`      TEXT,
          [create]      TEXT
      ) STRICT;
      "#
    );

    let parsed = sqlite3_parse_into_statement(&statement).unwrap().unwrap();

    let table: Table = parsed.try_into().unwrap();
    assert_eq!(table.name, "table");
    let sql = table.create_table_statement();

    assert_eq!(
      "CREATE TABLE 'table' ('index' TEXT, 'delete' TEXT, 'create' TEXT) STRICT",
      sql
    );
    sqlite3_parse_into_statement(&sql).unwrap().unwrap();
  }

  #[tokio::test]
  async fn test_statement_to_table_schema_and_back() {
    let statement = format!(
      r#"
      CREATE TABLE test (
          id                           BLOB PRIMARY KEY DEFAULT (uuid_v7()) NOT NULL,
          user                         BLOB DEFAULT '' REFERENCES _user(id),
          user_id                      BLOB,
          email                        TEXT NOT NULL,
          email_visibility             INTEGER DEFAULT FALSE NOT NULL,
          username                     TEXT,
          age                          INTEGER,
          double_age                   INTEGER GENERATED ALWAYS AS (2*age) VIRTUAL,
          triple_age                   INTEGER AS (3*age) STORED,

          UNIQUE (email),
          FOREIGN KEY(user_id) REFERENCES {USER_TABLE}(id) ON DELETE CASCADE
      ) STRICT;
      "#
    );

    {
      // First Make sure the query is actually valid, as opposed to "only" parsable.
      let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
      conn.execute(&statement, ()).await.unwrap();
    }

    let statement1 = sqlite3_parse_into_statement(&statement).unwrap().unwrap();
    let table1: Table = statement1.clone().try_into().unwrap();

    let sql = table1.create_table_statement();
    {
      // Same as above, make sure the constructed query is valid as opposed to "only" parsable.
      let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
      conn.execute(&sql, ()).await.unwrap();
    }

    let statement2 = sqlite3_parse_into_statement(&sql).unwrap().unwrap();

    let table2: Table = statement2.clone().try_into().unwrap();

    assert_eq!(statement1, statement2);
    assert_eq!(table1, table2);
  }

  #[test]
  fn test_statement_to_table_index_and_back() {
    const SQL: &str =
      "CREATE UNIQUE INDEX IF NOT EXISTS 'index' ON 'table' ('create') WHERE 'create' != '';";

    let statement1 = sqlite3_parse_into_statement(SQL).unwrap().unwrap();
    let index1: TableIndex = statement1.clone().try_into().unwrap();

    let statement2 = sqlite3_parse_into_statement(&index1.create_index_statement())
      .unwrap()
      .unwrap();
    let index2: TableIndex = statement2.clone().try_into().unwrap();

    assert_eq!(statement1, statement2);
    assert_eq!(index1, index2);
  }

  #[test]
  fn test_parse_create_trigger() {
    const SQL: &str = r#"
      CREATE TRIGGER cust_addr_chng
      INSTEAD OF UPDATE OF cust_addr ON customer_address
      FOR EACH ROW
      BEGIN
        UPDATE customer SET cust_addr=NEW.cust_addr WHERE cust_id=NEW.cust_id;
      END
    "#;

    sqlite3_parse_into_statement(SQL).unwrap().unwrap();
  }

  #[test]
  fn test_parse_create_index() {
    let sql = "CREATE UNIQUE INDEX index_name ON table_name(a ASC, b DESC) WHERE x > 0";
    let stmt = sqlite3_parse_into_statement(sql).unwrap().unwrap();
    let index: TableIndex = stmt.clone().try_into().unwrap();

    let sql1 = index.create_index_statement();
    let stmt1 = sqlite3_parse_into_statement(&sql1).unwrap().unwrap();

    assert_eq!(stmt, stmt1);
  }

  #[test]
  fn test_view_column_extraction() {
    let sql = "SELECT user, *, a.*, p.user AS foo FROM articles AS a LEFT JOIN profiles AS p ON p.user = a.author";
    let sqlite3_parser::ast::Stmt::Select(select) =
      sqlite3_parse_into_statement(sql).unwrap().unwrap()
    else {
      panic!("Not a select");
    };

    let tables = vec![
      Table {
        name: "profiles".to_string(),
        strict: true,
        columns: vec![
          Column {
            name: "user".to_string(),
            data_type: ColumnDataType::Blob,
            options: vec![
              ColumnOption::Unique { is_primary: true },
              ColumnOption::ForeignKey {
                foreign_table: "_user".to_string(),
                referred_columns: vec!["id".to_string()],
                on_delete: None,
                on_update: None,
              },
            ],
          },
          Column {
            name: "username".to_string(),
            data_type: ColumnDataType::Text,
            options: vec![],
          },
        ],
        foreign_keys: vec![],
        unique: vec![],
        virtual_table: false,
        temporary: false,
      },
      Table {
        name: "articles".to_string(),
        strict: true,
        columns: vec![
          Column {
            name: "id".to_string(),
            data_type: ColumnDataType::Blob,
            options: vec![ColumnOption::Unique { is_primary: true }],
          },
          Column {
            name: "author".to_string(),
            data_type: ColumnDataType::Blob,
            options: vec![ColumnOption::ForeignKey {
              foreign_table: "_user".to_string(),
              referred_columns: vec!["id".to_string()],
              on_delete: None,
              on_update: None,
            }],
          },
          Column {
            name: "body".to_string(),
            data_type: ColumnDataType::Text,
            options: vec![],
          },
        ],
        foreign_keys: vec![],
        unique: vec![],
        virtual_table: false,
        temporary: false,
      },
    ];

    let mapping = try_extract_column_mapping(*select, &tables)
      .unwrap()
      .unwrap();

    assert_eq!(
      mapping
        .iter()
        .map(|m| m.referred_column.as_ref().unwrap().column_name.as_str())
        .collect::<Vec<_>>(),
      ["user", "id", "author", "body", "user", "username", "id", "author", "body", "user"]
    );

    assert_eq!(
      mapping
        .iter()
        .map(|m| m.column.name.as_str())
        .collect::<Vec<_>>(),
      ["user", "id", "author", "body", "user", "username", "id", "author", "body", "foo"]
    );
  }
}
