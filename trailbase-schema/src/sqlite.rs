use fallible_iterator::FallibleIterator;
use itertools::Itertools;
use log::*;
use serde::{Deserialize, Serialize};
use sqlite3_parser::ast::{
  ColumnDefinition, CreateTableBody, DeferSubclause, Expr, ForeignKeyClause, FromClause,
  IndexedColumn, Literal, Name, QualifiedName as AstQualifiedName, SelectTable, Stmt,
  TableConstraint, TableOptions, fmt::ToTokens,
};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum SchemaError {
  #[error("Missing ObjectName")]
  MissingName,
  #[error("Precondition failed: {0}")]
  Precondition(Box<dyn std::error::Error + Send + Sync>),
}

pub fn sqlite3_parse_into_statements(
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

pub fn sqlite3_parse_into_statement(
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

  // Only "ON DELETE" and "ON UPDATE" are supported in foreign key clause, i.e. no "ON INSERT":
  //   https://www.sqlite.org/syntax/foreign-key-clause.html
  pub on_delete: Option<ReferentialAction>,
  pub on_update: Option<ReferentialAction>,
  // TODO: Missing DEFERRABLE.
}

impl ForeignKey {
  fn to_fragment(&self) -> String {
    let cols = quote(&self.columns);
    let foreign_table = &self.foreign_table;
    let ref_col = match self.referred_columns.len() {
      0 => "".to_string(),
      _ => format!("({})", quote(&self.referred_columns)),
    };

    let on_delete = self.on_delete.as_ref().map_or_else(
      || "".to_string(),
      |action| format!("ON DELETE {}", action.to_fragment()),
    );
    let on_update = self.on_update.as_ref().map_or_else(
      || "".to_string(),
      |action| format!("ON UPDATE {}", action.to_fragment()),
    );

    return if let Some(ref name) = self.name {
      format!(
        "CONSTRAINT '{name}' FOREIGN KEY ({cols}) REFERENCES '{foreign_table}'{ref_col} {on_delete} {on_update}"
      )
    } else {
      format!("FOREIGN KEY ({cols}) REFERENCES '{foreign_table}'{ref_col} {on_delete} {on_update}")
    };
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct Check {
  pub name: Option<String>,
  pub expr: String,
}

impl Check {
  fn to_fragment(&self) -> String {
    return if let Some(ref name) = self.name {
      format!("CONSTRAINT '{name}' CHECK({})", self.expr)
    } else {
      format!("CHECK({})", self.expr)
    };
  }
}

// https://www.sqlite.org/syntax/table-constraint.html.
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct UniqueConstraint {
  pub name: Option<String>,

  /// Identifiers of the columns that are unique.
  ///
  /// TODO: Should be indexed/ordered column, e.g. ASC/DESC:
  ///   https://www.sqlite.org/syntax/indexed-column.html
  pub columns: Vec<String>,

  pub conflict_clause: Option<ConflictResolution>,
}

impl UniqueConstraint {
  fn to_fragment(&self) -> String {
    let cols = quote(&self.columns);

    return match (self.name.as_ref(), &self.conflict_clause.as_ref()) {
      (Some(name), Some(resolution)) => format!(
        "CONSTRAINT '{name}' UNIQUE ({cols}) ON CONFLICT {}",
        resolution.to_fragment()
      ),
      (Some(name), None) => format!("CONSTRAINT '{name}' UNIQUE ({cols})"),
      (None, Some(resolution)) => {
        format!("UNIQUE ({cols}) ON CONFLICT {}", resolution.to_fragment())
      }
      (None, None) => format!("UNIQUE ({cols})"),
    };
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct ColumnOrder {
  pub column_name: String,
  pub ascending: Option<bool>,
  pub nulls_first: Option<bool>,
}

/// Conflict resolution types
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub enum ConflictResolution {
  /// `ROLLBACK`
  Rollback,
  /// `ABORT`
  Abort, // default
  /// `FAIL`
  Fail,
  /// `IGNORE`
  Ignore,
  /// `REPLACE`
  Replace,
}

impl From<sqlite3_parser::ast::ResolveType> for ConflictResolution {
  fn from(res: sqlite3_parser::ast::ResolveType) -> Self {
    use sqlite3_parser::ast::ResolveType;
    match res {
      ResolveType::Rollback => ConflictResolution::Rollback,
      ResolveType::Abort => ConflictResolution::Abort,
      ResolveType::Fail => ConflictResolution::Fail,
      ResolveType::Ignore => ConflictResolution::Ignore,
      ResolveType::Replace => ConflictResolution::Replace,
    }
  }
}

impl ConflictResolution {
  // https://www.sqlite.org/syntax/conflict-clause.html
  fn to_fragment(&self) -> &'static str {
    return match self {
      Self::Rollback => "ROLLBACK",
      Self::Abort => "ABORT",
      Self::Fail => "FAIL",
      Self::Ignore => "IGNORE",
      Self::Replace => "REPLACE",
    };
  }
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
    conflict_clause: Option<ConflictResolution>,
    // TODO: Missing ASC/DESC & AUTOINCREMENT for PK.
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
      Self::Unique {
        is_primary,
        conflict_clause,
      } => match (*is_primary, conflict_clause.as_ref()) {
        (true, Some(res)) => format!("PRIMARY KEY ON CONFLICT {}", res.to_fragment()),
        (true, None) => "PRIMARY KEY".to_string(),
        (false, Some(res)) => format!("UNIQUE ON CONFLICT {}", res.to_fragment()),
        (false, None) => "UNIQUE".to_string(),
      },
      Self::ForeignKey {
        foreign_table,
        referred_columns,
        on_delete,
        on_update,
      } => {
        format!(
          "REFERENCES '{foreign_table}'{ref_col} {on_delete} {on_update}",
          ref_col = match referred_columns.len() {
            0 => "".to_string(),
            _ => format!("({})", quote(referred_columns)),
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
      Self::OnUpdate(expr) => expr.clone(),
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

  pub fn is_not_null(&self) -> bool {
    return self
      .options
      .iter()
      .any(|opt| matches!(opt, ColumnOption::NotNull));
  }

  pub fn has_default(&self) -> bool {
    return self
      .options
      .iter()
      .any(|opt| matches!(opt, ColumnOption::Default(_)));
  }

  pub fn is_primary(&self) -> bool {
    return self.options.iter().any(
      |opt| matches!(opt, ColumnOption::Unique { is_primary, conflict_clause: _ } if *is_primary ),
    );
  }
}

#[derive(Clone, Default, Debug, Serialize, Deserialize, TS)]
pub struct QualifiedName {
  pub name: String,
  pub database_schema: Option<String>,
}

impl QualifiedName {
  pub fn parse(name: &str) -> Self {
    if let Some((db, name)) = name.split_once('.') {
      return Self {
        name: name.to_string(),
        database_schema: Some(db.to_string()),
      };
    }
    return Self {
      name: name.to_string(),
      database_schema: None,
    };
  }

  pub fn escaped_string(&self) -> String {
    return if let Some(ref db) = self.database_schema {
      format!(r#""{db}"."{}""#, self.name)
    } else {
      format!(r#""{}""#, self.name)
    };
  }
}

impl PartialEq for QualifiedName {
  fn eq(&self, other: &Self) -> bool {
    return self.name == other.name
      && self.database_schema.as_deref().unwrap_or("main")
        == other.database_schema.as_deref().unwrap_or("main");
  }
}

impl Eq for QualifiedName {}

impl Hash for QualifiedName {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.name.hash(state);
    self
      .database_schema
      .as_deref()
      .unwrap_or("main")
      .hash(state);
  }
}

impl From<AstQualifiedName> for QualifiedName {
  fn from(qn: AstQualifiedName) -> Self {
    return Self {
      database_schema: unquote_db_name(&qn),
      name: unquote_qualified(qn),
    };
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Table {
  pub name: QualifiedName,
  pub strict: bool,

  // Column definition and column-level constraints.
  pub columns: Vec<Column>,

  // Table-level constraints, e.g. composite uniqueness or foreign keys. Columns may have their own
  // column-level constraints a.k.a. Column::options.
  pub foreign_keys: Vec<ForeignKey>,
  pub unique: Vec<UniqueConstraint>,
  pub checks: Vec<Check>,

  // NOTE: consider parsing "CREATE VIRTUAL TABLE" into a separate struct.
  pub virtual_table: bool,
  pub temporary: bool,
}

impl Table {
  pub fn create_table_statement(&self) -> String {
    if self.virtual_table {
      // https://www.sqlite.org/lang_createvtab.html
      panic!("Not implemented");
    }

    let mut column_defs_and_table_constraints: Vec<String> = vec![];

    column_defs_and_table_constraints.extend(self.columns.iter().map(|c| c.to_fragment()));

    // Example: UNIQUE (email),
    column_defs_and_table_constraints.extend(self.unique.iter().map(|unique| unique.to_fragment()));

    // Example: FOREIGN KEY(user_id) REFERENCES table(id) ON DELETE CASCADE
    column_defs_and_table_constraints.extend(self.foreign_keys.iter().map(|fk| fk.to_fragment()));

    // Example: CHECK('age' > 0)
    column_defs_and_table_constraints.extend(self.checks.iter().map(|fk| fk.to_fragment()));

    return format!(
      "CREATE{temporary} TABLE {fq_name} ({col_defs_and_constraints}){strict}",
      temporary = if self.temporary { " TEMPORARY" } else { "" },
      fq_name = self.name.escaped_string(),
      col_defs_and_constraints = column_defs_and_table_constraints.join(", "),
      strict = if self.strict { " STRICT" } else { "" },
    );
  }
}

#[derive(Clone, Default, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct TableIndex {
  pub name: QualifiedName,

  pub table_name: String,
  pub columns: Vec<ColumnOrder>,
  pub unique: bool,
  pub predicate: Option<String>,

  #[ts(skip)]
  #[serde(default)]
  pub if_not_exists: bool,
}

impl TableIndex {
  pub fn create_index_statement(&self) -> String {
    let indexed_columns = self
      .columns
      .iter()
      .map(|c| {
        format!(
          "'{name}' {order}",
          name = c.column_name,
          order = c
            .ascending
            .map_or("", |asc| if asc { "ASC" } else { "DESC" })
        )
      })
      .join(", ");

    return format!(
      "CREATE{unique} INDEX {if_not_exists} {fqn_name} ON '{table_name}' ({indexed_columns}) {predicate}",
      unique = if self.unique { " UNIQUE" } else { "" },
      if_not_exists = if self.if_not_exists {
        "IF NOT EXISTS"
      } else {
        ""
      },
      fqn_name = self.name.escaped_string(),
      table_name = self.table_name,
      predicate = self
        .predicate
        .as_ref()
        .map_or_else(|| "".to_string(), |p| format!("WHERE {p}")),
    );
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct View {
  pub name: QualifiedName,

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

        let mut foreign_keys: Vec<ForeignKey> = vec![];
        let mut unique: Vec<UniqueConstraint> = vec![];
        let mut checks: Vec<Check> = vec![];

        for constraint in constraints.unwrap_or_default() {
          match constraint.constraint {
            TableConstraint::ForeignKey {
              columns,
              clause,
              deref_clause,
            } => {
              foreign_keys.push(build_foreign_key(
                constraint.name,
                Some(columns),
                clause,
                deref_clause,
              ));
            }
            TableConstraint::Unique {
              columns,
              conflict_clause,
            } => {
              unique.push(UniqueConstraint {
                name: constraint.name.map(unquote_name),
                columns: columns.into_iter().map(|c| unquote_expr(c.expr)).collect(),
                conflict_clause: conflict_clause.map(|c| c.into()),
              });
            }
            TableConstraint::Check(expr) => {
              checks.push(Check {
                name: constraint.name.map(unquote_name),
                expr: expr.to_string(),
              });
            }
            TableConstraint::PrimaryKey { .. } => {
              warn!("PK table constraint not implemented. Use column constraints.");
            }
          }
        }

        let columns: Vec<Column> = columns
          .into_iter()
          .map(|(name, def): (Name, ColumnDefinition)| {
            let ColumnDefinition {
              col_name,
              col_type,
              constraints,
            } = def;
            assert_eq!(name, col_name);

            let name = unquote_name(col_name);
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

        Ok(Table {
          name: tbl_name.into(),
          strict: options.contains(TableOptions::STRICT),
          columns,
          foreign_keys,
          unique,
          checks,
          virtual_table: false,
          temporary,
        })
      }
      Stmt::CreateVirtualTable {
        tbl_name,
        args: _args,
        ..
      } => Ok(Table {
        name: tbl_name.into(),
        strict: false,
        columns: vec![],
        foreign_keys: vec![],
        unique: vec![],
        checks: vec![],
        virtual_table: true,
        temporary: false,
      }),
      _ => Err(SchemaError::Precondition(
        format!("expected 'CREATE [VIRTUAL] TABLE', got: {value:?}").into(),
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
        conflict_clause,
        order: _,
        auto_increment: _,
      } => ColumnOption::Unique {
        is_primary: true,
        conflict_clause: conflict_clause.map(|c| c.into()),
      },
      Constraint::Unique(conflict_clause) => ColumnOption::Unique {
        is_primary: false,
        conflict_clause: conflict_clause.map(|c| c.into()),
      },
      Constraint::Check(expr) => {
        // NOTE: This is not using unquote on purpose, since this is not an identifier.
        ColumnOption::Check(expr.to_string())
      }
      Constraint::ForeignKey {
        clause,
        deref_clause,
      } => {
        let fk = build_foreign_key(None, None, clause, deref_clause);

        ColumnOption::ForeignKey {
          foreign_table: fk.foreign_table,
          referred_columns: fk.referred_columns,
          on_delete: fk.on_delete,
          on_update: fk.on_update,
        }
      }
      Constraint::NotNull { .. } => ColumnOption::NotNull,
      Constraint::Default(expr) => {
        // NOTE: This is not using unquote on purpose to avoid turning "DEFAULT ''" into "DEFAULT".
        ColumnOption::Default(expr.to_string())
      }
      Constraint::Generated { expr, typ } => ColumnOption::Generated {
        // NOTE: This is not using unquote on purpose to avoid turning "AS ('')" into "AS ()".
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
        name: idx_name.into(),
        table_name: unquote_name(tbl_name),
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
        predicate: where_clause.map(|clause| {
          // NOTE: this is deliberately not unquoting.
          clause.to_string()
        }),
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
          name: view_name.into(),
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
  qn: AstQualifiedName,
  alias: Option<sqlite3_parser::ast::As>,
) -> (String, QualifiedName) {
  return (
    alias
      .and_then(|alias| {
        if let sqlite3_parser::ast::As::As(name) = alias {
          return Some(unquote_name(name));
        }
        None
      })
      .unwrap_or_else(|| qn.to_string()),
    qn.into(),
  );
}

#[derive(Clone, Debug)]
#[allow(unused)]
struct ReferredColumn {
  table_name: QualifiedName,
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
  let mut table_names = indexmap::IndexMap::<String, QualifiedName>::from([to_entry(fqn, alias)]);

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
  let all_tables: HashMap<QualifiedName, &Table> =
    tables.iter().map(|t| (t.name.clone(), t)).collect();
  let mut all_columns = HashMap::<String, (&Table, &Column)>::new();

  // Make sure we know all tables and all tables are strict.
  for table_name in table_names.values() {
    match all_tables.get(table_name) {
      Some(table) => {
        if !table.strict {
          info!("Skipping view: referenced table: {table_name:?} not strict");
          return Ok(None);
        }

        for col in &table.columns {
          all_columns.insert(col.name.clone(), (table, col));
        }
      }
      None => {
        return Err(SchemaError::Precondition(
          format!("View's SELECT references missing table: {table_name:?}").into(),
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
        let name = unquote_name(name);
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
          let col_name = unquote_id(id.clone());
          let Some((table, column)) = all_columns.get(&col_name) else {
            return Err(SchemaError::Precondition(
              format!("Missing columns: {id:?}").into(),
            ));
          };

          let name = alias
            .and_then(|alias| {
              if let sqlite3_parser::ast::As::As(name) = alias {
                return Some(unquote_name(name));
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
          let qualifier = unquote_name(qualifier);
          let col_name = unquote_name(name);

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
                return Some(unquote_name(name));
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
              return Some(unquote_name(name));
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

fn build_foreign_key(
  name: Option<Name>,
  columns: Option<Vec<IndexedColumn>>,
  clause: ForeignKeyClause,
  deref_clause: Option<DeferSubclause>,
) -> ForeignKey {
  if let Some(ref clause) = deref_clause {
    // TOOD: Parse DEFERRABLE.
    warn!("Unsupported DEFERRABLE in FK clause: {clause:?}");
  }

  let (on_update, on_delete) = unparse_fk_trigger(&clause.args);

  return ForeignKey {
    name: name.map(unquote_name),
    foreign_table: unquote_name(clause.tbl_name.clone()),
    columns: columns
      .unwrap_or_default()
      .into_iter()
      .map(|c| unquote_name(c.col_name))
      .collect(),
    referred_columns: clause
      .columns
      .unwrap_or_default()
      .into_iter()
      .map(|c| unquote_name(c.col_name))
      .collect(),
    on_update,
    on_delete,
  };
}

fn unparse_fk_trigger(
  args: &Vec<sqlite3_parser::ast::RefArg>,
) -> (Option<ReferentialAction>, Option<ReferentialAction>) {
  use sqlite3_parser::ast::RefArg;

  let mut on_update: Option<ReferentialAction> = None;
  let mut on_delete: Option<ReferentialAction> = None;

  for arg in args {
    match arg {
      RefArg::OnDelete(action) => {
        on_delete = Some((*action).into());
      }
      RefArg::OnUpdate(action) => {
        on_update = Some((*action).into());
      }
      RefArg::OnInsert(action) => {
        error!("Unexpected ON INSERT in FK clause: {action:?}");
      }
      RefArg::Match(name) => {
        // SQL supports FK MATCH clause, which is *not* supported by sqlite:
        //   https://www.sqlite.org/foreignkeys.html#fk_unsupported
        warn!("Unsupported MATCH in FK clause: {name:?}");
      }
    }
  }

  return (on_update, on_delete);
}

#[inline]
pub(crate) fn quote(column_names: &[String]) -> String {
  let mut s = String::new();
  for (i, name) in column_names.iter().enumerate() {
    if i > 0 {
      s.push_str(", '");
    } else {
      s.push('\'');
    }
    s.push_str(name);
    s.push('\'');
  }
  return s;
}

#[inline]
fn unquote_string(s: String) -> String {
  let n = s.as_bytes();
  if n.is_empty() {
    return s;
  }

  return match n[0] {
    b'"' | b'`' | b'\'' | b'[' => {
      assert!(n.len() >= 2, "string: {s}");
      s[1..n.len() - 1].to_string()
    }
    _ => s,
  };
}

fn unquote_name(name: Name) -> String {
  return unquote_string(name.0);
}

fn unquote_qualified(name: AstQualifiedName) -> String {
  return unquote_name(name.name);
}

fn unquote_db_name(name: &AstQualifiedName) -> Option<String> {
  return name.db_name.clone().map(unquote_name);
}

fn unquote_id(id: sqlite3_parser::ast::Id) -> String {
  return unquote_string(id.0);
}

fn unquote_expr(expr: Expr) -> String {
  return match expr {
    Expr::Name(n) => unquote_name(n),
    Expr::Id(id) => unquote_id(id),
    Expr::Literal(Literal::String(s)) => unquote_string(s),
    x => x.to_string(),
  };
}

#[cfg(test)]
pub fn lookup_and_parse_table_schema(
  conn: &rusqlite::Connection,
  table_name: &str,
) -> anyhow::Result<Table> {
  const SQLITE_SCHEMA_TABLE: &str = "main.sqlite_schema";

  let sql: String = conn.query_row(
    &format!("SELECT sql FROM {SQLITE_SCHEMA_TABLE} WHERE type = 'table' AND name = $1"),
    rusqlite::params!(table_name),
    |row| row.get(0),
  )?;

  let Some(stmt) = sqlite3_parse_into_statement(&sql)? else {
    anyhow::bail!("Not a statement");
  };

  return Ok(stmt.try_into()?);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_quote() {
    assert_eq!("", quote(&vec![]));
    assert_eq!("''", quote(&vec!["".to_string()]));
    assert_eq!("'foo', ''", quote(&vec!["foo".to_string(), "".to_string()]));
  }

  #[test]
  fn test_unquote() {
    assert_eq!(unquote_name(Name("".to_string())), "");
    assert_eq!(unquote_name(Name("['``']".to_string())), "'``'");
    assert_eq!(unquote_name(Name("\"[]\"".to_string())), "[]");
  }

  #[test]
  fn test_create_table_statement_quoting() {
    let table_name = QualifiedName {
      name: "table".to_string(),
      database_schema: None,
    };
    let statement = format!(
      r#"
      CREATE TABLE {table_name} (
          'index'       TEXT,
          `delete`      TEXT,
          [create]      TEXT
      ) STRICT;
      "#,
      table_name = table_name.escaped_string(),
    );

    let parsed = sqlite3_parse_into_statement(&statement).unwrap().unwrap();

    let table: Table = parsed.try_into().unwrap();
    assert_eq!(table.name, table_name);
    let sql = table.create_table_statement();

    assert_eq!(
      "CREATE TABLE \"table\" ('index' TEXT, 'delete' TEXT, 'create' TEXT) STRICT",
      sql
    );
    sqlite3_parse_into_statement(&sql).unwrap().unwrap();
  }

  struct StmtFormatter(Stmt);

  impl std::fmt::Display for StmtFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      self.0.to_fmt(f)
    }
  }

  #[tokio::test]
  async fn test_statement_to_table_schema_and_back() {
    let statement = format!(
      r#"
      CREATE TABLE test (
          -- Comment
          id                           BLOB PRIMARY KEY DEFAULT (uuid_v7()) NOT NULL,
          user                         BLOB DEFAULT '' REFERENCES 'table'(`index`) ON DELETE CASCADE,
          user_id                      BLOB,
          email                        TEXT NOT NULL,
          email_visibility             INTEGER DEFAULT FALSE NOT NULL,
          username                     TEXT UNIQUE ON CONFLICT ABORT,
          age                          INTEGER CHECK(age >= 0),
          double_age                   INTEGER GENERATED ALWAYS AS (2 * 'age') VIRTUAL,
          triple_age                   INTEGER AS (3 * age) STORED,
          gen_text                     TEXT AS ('') VIRTUAL,
          [index]                      TEXT,

          UNIQUE (email),
          -- optional constraint name:
          CONSTRAINT `unique` UNIQUE ([index]) ON CONFLICT FAIL,
          FOREIGN KEY(user_id) REFERENCES 'table'('index') ON DELETE CASCADE,
          CONSTRAINT `check` CHECK(username != '')
      ) STRICT;
      "#
    );

    {
      // First Make sure the query is actually valid, as opposed to "only" parsable.
      let conn = trailbase_extension::connect_sqlite(None, None).unwrap();
      conn.execute(&statement, ()).unwrap();
    }

    let statement1 = sqlite3_parse_into_statement(&statement).unwrap().unwrap();
    let table1: Table = statement1.clone().try_into().unwrap();

    let sql = table1.create_table_statement();
    {
      // Same as above, make sure the constructed query is valid as opposed to "only" parsable.
      let conn = trailbase_extension::connect_sqlite(None, None).unwrap();
      conn.execute(&sql, ()).unwrap();
    }

    let statement2 = sqlite3_parse_into_statement(&sql).unwrap().unwrap();

    let table2: Table = statement2.clone().try_into().unwrap();

    // NOTE: Ideally we'd just compare the parsed sqlite3_parser ASTs, however it doesn't properly
    // parse out escape characters, so `statement1` and `statement2` will be escaped differently.
    // So we're matching on strings instead with all quoting removed.
    // assert_eq!(statement1, statement2, "Got: {sql2}\nExpected: {sql1}");
    let pattern = ['\'', '"', '[', ']', '`'];
    let sql2 = StmtFormatter(statement2.clone())
      .to_string()
      .replace(&pattern, "");
    let sql1 = StmtFormatter(statement1.clone())
      .to_string()
      .replace(&pattern, "");
    assert_eq!(sql2, sql1, "Got: {sql2}\nExpected: {sql1}");

    assert_eq!(table1, table2, "generated stmt: {sql}");
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
    let sql =
      r#"CREATE UNIQUE INDEX "main"."index_name" ON 'table_name' (a ASC, b DESC) WHERE x > 0"#;
    let stmt = sqlite3_parse_into_statement(sql).unwrap().unwrap();
    let index: TableIndex = stmt.try_into().unwrap();

    let sql1 = index.create_index_statement();
    let stmt1 = sqlite3_parse_into_statement(&sql1).unwrap().unwrap();
    let index1: TableIndex = stmt1.try_into().unwrap();

    assert_eq!(index, index1, "Parsed: {sql1}");
  }

  #[test]
  fn test_view_column_extraction() {
    let sql = "SELECT user, *, a.*, p.user AS foo FROM foo.articles AS a LEFT JOIN bar.profiles AS p ON p.user = a.author";
    let sqlite3_parser::ast::Stmt::Select(select) =
      sqlite3_parse_into_statement(sql).unwrap().unwrap()
    else {
      panic!("Not a select");
    };

    let tables = vec![
      Table {
        name: QualifiedName {
          name: "profiles".to_string(),
          database_schema: Some("bar".to_string()),
        },
        strict: true,
        columns: vec![
          Column {
            name: "user".to_string(),
            data_type: ColumnDataType::Blob,
            options: vec![
              ColumnOption::Unique {
                is_primary: true,
                conflict_clause: None,
              },
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
        checks: vec![],
        virtual_table: false,
        temporary: false,
      },
      Table {
        name: QualifiedName {
          name: "articles".to_string(),
          database_schema: Some("foo".to_string()),
        },
        strict: true,
        columns: vec![
          Column {
            name: "id".to_string(),
            data_type: ColumnDataType::Blob,
            options: vec![ColumnOption::Unique {
              is_primary: true,
              conflict_clause: None,
            }],
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
        checks: vec![],
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
      [
        "user", "id", "author", "body", "user", "username", "id", "author", "body", "user"
      ]
    );

    assert_eq!(
      mapping
        .iter()
        .map(|m| m.column.name.as_str())
        .collect::<Vec<_>>(),
      [
        "user", "id", "author", "body", "user", "username", "id", "author", "body", "foo"
      ]
    );
  }
}
