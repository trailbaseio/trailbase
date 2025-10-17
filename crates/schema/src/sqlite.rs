/// This file contains table schema and index representations. Originally, they were mostly
/// adaptations of sqlparser's CreateX AST representations (we've since moved to
/// sqlite3_parser). This serves two purposes:
///
///  * We'd like some representation that we can construct on the client with type-safety. We
///    could also consider using proto here, but ts-rs let's us "skip" some fields.
///  * But also, there's a fundamental difference between an AST that represents a specific SQL
///    program and a more abstract semantic representation of the schema, e.g. we don't care in
///    which order indexes were constructed or what quotes were used...
///
/// NOTE: We're very much "over-wrapping" here entering the space of the exact-program AST
/// domain. This is mostly convenient for testing our code by transforming back and forth and
/// checking the output is stable. We can use "skip" to remove some more "representational"
/// details from the API.
use itertools::Itertools;
use log::*;
use serde::{Deserialize, Serialize};
use sqlite3_parser::ast::{
  ColumnDefinition, CreateTableBody, DeferSubclause, Expr, ForeignKeyClause, FromClause,
  IndexedColumn, JoinOperator, JoinType, Literal, Name, QualifiedName as AstQualifiedName,
  ResultColumn, SelectTable, Stmt, TabFlags, TableConstraint, fmt::ToTokens,
};
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
      Constraint::NotNull {
        nullable,
        conflict_clause: _,
      } => match nullable {
        true => ColumnOption::Null,
        false => ColumnOption::NotNull,
      },
      Constraint::Default(expr) => {
        // NOTE: This is not using unquote on purpose to avoid turning "DEFAULT ''" into "DEFAULT".
        ColumnOption::Default(expr.to_string())
      }
      Constraint::Generated { expr, typ } => ColumnOption::Generated {
        // NOTE: This is not using unquote on purpose to avoid turning "AS ('')" into "AS ()".
        expr: expr.to_string(),
        mode: typ.and_then(|t| match &*t.0 {
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS, PartialEq)]
pub enum ColumnDataType {
  // Per cell storage type.
  Any,
  // Strict storage types.
  Blob,
  Text,
  // Can either be specified as "INT" or "INTEGER".
  Integer,
  Real,
}

impl ColumnDataType {
  fn from_type_name(type_name: &str) -> Option<Self> {
    return Some(match type_name.to_uppercase().as_str() {
      "ANY" => Self::Any,
      "BLOB" => Self::Blob,
      "TEXT" => Self::Text,
      "INTEGER" => Self::Integer,
      // INT is the only allowed alias: https://sqlite.org/stricttables.html.
      "INT" => Self::Integer,
      "REAL" => Self::Real,
      _ => {
        debug!("Unexpected data type: {type_name:?}");
        return None;
      }
    });
  }

  pub(crate) fn is_integer_kind(&self) -> bool {
    return matches!(self, Self::Integer);
  }

  pub(crate) fn is_float_kind(&self) -> bool {
    return matches!(self, Self::Real);
  }
}

// impl From<sqlite3_parser::ast::Type> for ColumnDataType {
//   fn from(data_type: sqlite3_parser::ast::Type) -> Self {
//     // NOTE: In STRICT mode only explicit column data types are allowed (also INT is an alias for
//     // INTEGER). Otherwise, any type-name goes, e.g. "FOO". There are rules to also derive
//     // affinities, if certain substrings are contained, e.g. "FOOINTBAR" will have INTEGER
// affinity     return
// ColumnDataType::from_type_name(&data_type.name).unwrap_or(ColumnDataType::Any);   }
// }

/// Different affinity types in SQLite will lead to different preferences in interpreting input
/// literals. For example, a column with REAL preference, will store any input but try to convert
/// strings into REAL when possible.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS, PartialEq)]
pub enum ColumnAffinityType {
  Text,
  Integer,
  Real,
  Blob,
  // Will try to convert '5' to INTEGER and '5.6' to REAL storage, otherwise TEXT.
  Numeric,
}

impl ColumnAffinityType {
  pub fn from_data_type(data_type: ColumnDataType) -> Self {
    match data_type {
      ColumnDataType::Any => Self::Numeric,
      ColumnDataType::Integer => Self::Integer,
      ColumnDataType::Real => Self::Real,
      ColumnDataType::Blob => Self::Blob,
      ColumnDataType::Text => Self::Text,
    }
  }

  pub fn from_type_name(type_name: &str) -> Self {
    // Affinity types are derived from 5 rules described here: https://sqlite.org/datatype3.html.
    let type_name = type_name.to_uppercase();

    // 1. If the declared type contains the string "INT" then it is assigned INTEGER affinity.
    if type_name.contains("INT") {
      return Self::Integer;
    }

    // 2. If the declared type of the column contains any of the strings "CHAR", "CLOB", or "TEXT"
    //    then that column has TEXT affinity. Notice that the type VARCHAR contains the string
    //    "CHAR" and is thus assigned TEXT affinity.
    if type_name.contains("CHAR") || type_name.contains("CLOB") || type_name.contains("TEXT") {
      return Self::Text;
    }

    // 3. If the declared type for a column contains the string "BLOB" or if no type is specified
    //    then the column has affinity BLOB.
    if type_name.contains("BLOB") || type_name.is_empty() {
      return Self::Blob;
    }

    // 4. If the declared type for a column contains any of the strings "REAL", "FLOA", or "DOUB"
    //    then the column has REAL affinity.
    if type_name.contains("REAL") || type_name.contains("FLOA") || type_name.contains("DOUB") {
      return Self::Real;
    }

    // 5. Otherwise, the affinity is NUMERIC.
    return Self::Numeric;
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct Column {
  pub name: String,
  pub type_name: String,
  pub data_type: ColumnDataType,
  pub affinity_type: ColumnAffinityType,
  pub options: Vec<ColumnOption>,
}

impl Column {
  fn to_fragment(&self) -> String {
    let options: Vec<String> = self.options.iter().map(|o| o.to_fragment()).collect();

    return if options.is_empty() {
      format!(
        "'{name}' {type_name}",
        name = self.name,
        type_name = self.type_name,
      )
    } else {
      format!(
        "'{name}' {type_name} {options}",
        name = self.name,
        type_name = self.type_name,
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
  pub fn parse(name: &str) -> Result<Self, SchemaError> {
    if name.contains(';') {
      return Err(SchemaError::Precondition("Invalid name".into()));
    }

    if let Some((db, name)) = name.split_once('.') {
      return Ok(Self {
        name: unquote_string(name),
        database_schema: Some(unquote_string(db)),
      });
    }
    return Ok(Self {
      name: unquote_string(name),
      database_schema: None,
    });
  }

  pub fn escaped_string(&self) -> String {
    return if let Some(ref db) = self.database_schema {
      format!(r#""{db}"."{}""#, self.name)
    } else {
      format!(r#""{}""#, self.name)
    };
  }

  pub fn migration_filename(&self, prefix: &str) -> String {
    return if let Some(ref db) = self.database_schema {
      format!("{prefix}_{db}_{}", self.name)
    } else {
      format!("{prefix}_{}", self.name)
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
          flags,
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
              flags: _,
            } = def;
            assert_eq!(name, col_name);

            let name = unquote_name(col_name);
            assert!(!name.is_empty());

            let (type_name, affinity_type, data_type): (
              String,
              ColumnAffinityType,
              ColumnDataType,
            ) = match col_type {
              Some(x) => (
                x.name.clone(),
                ColumnAffinityType::from_type_name(&x.name),
                ColumnDataType::from_type_name(&x.name).unwrap_or(ColumnDataType::Any),
              ),
              None => (
                "".to_string(),
                ColumnAffinityType::from_type_name(""),
                ColumnDataType::Any,
              ),
            };

            let options: Vec<ColumnOption> = constraints
              .into_iter()
              .map(|named_constraint| named_constraint.constraint.into())
              .collect();

            return Column {
              name,
              type_name,
              data_type,
              affinity_type,
              options,
            };
          })
          .collect();

        Ok(Table {
          name: tbl_name.into(),
          strict: flags.contains(TabFlags::Strict),
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

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct View {
  pub name: QualifiedName,

  /// Columns may be inferred from a view's query.
  ///
  /// Views can be defined with arbitrary queries referencing arbitrary sources: tables, views,
  /// functions, ..., which makes them inherently not type safe and therefore their columns not
  /// well defined.
  ///
  /// QUESTION: We've been wondering if the inference should live more in ViewMetadata, however
  /// right now the `View` is heavily used in the UI to e.g. render tables and infer record API
  /// suitability. It's ok that this is more than just an AST.
  pub(crate) column_mapping: Option<ColumnMapping>,

  pub query: String,

  pub temporary: bool,

  #[ts(skip)]
  pub if_not_exists: bool,
}

impl View {
  pub fn from(stmt: sqlite3_parser::ast::Stmt, tables: &[Table]) -> Result<Self, SchemaError> {
    let sqlite3_parser::ast::Stmt::CreateView {
      temporary,
      if_not_exists,
      view_name,
      columns,
      select,
    } = stmt
    else {
      return Err(SchemaError::Precondition(
        format!("expected 'CREATE VIEW', got: {stmt:?}").into(),
      ));
    };

    let column_mapping: Option<ColumnMapping> = if columns.is_some() {
      // Example, `CREATE VIEW view0(alias0, alias1) AS SELECT * FROM table0;`
      //
      // We probably never want to support this due to its late failure mode,
      // i.e. column mismatches are discovered at query-time rather than
      // view-creation. Also table schema changes may later invalidate
      // existing views.
      debug!("VIEW column aliases not supported for APIs");
      None
    } else {
      // Try to parse columns very liberally. We don't want to disallow complex
      // VIEWs but returning a `View` with `None` columns, means it cannot be used
      // for APIs.
      extract_column_mapping((*select).clone(), tables)
        .map_err(|err| {
          debug!(
            "Failed to extract VIEW column mapping from '{:?}': {err}",
            *select
          );
          return err;
        })
        .ok()
    };

    return Ok(View {
      name: view_name.into(),
      column_mapping,
      query: SelectFormatter(*select).to_string(),
      temporary,
      if_not_exists,
    });
  }
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub(crate) struct ViewColumn {
  // e.g. "foo" for CREATE VIEW v AS SELECT foo.bar AS baz FROM ...
  // #[allow(unused)]
  // pub(crate) qualifier: Option<String>,
  //
  // // e.g. "baz" for CREATE VIEW v AS SELECT foo.bar AS baz FROM ...
  // pub(crate) alias: Option<String>,

  // The inferred column schema, either via a cast from a computed column or the underlying table
  // column if inferable.
  // NOTE: It would be cleaner to separate (Table)`Column` from `ViewColumn`, just pulling the in
  // the contents here. However, the UI currently depends on `Column`.
  pub(crate) column: Column,

  // Would be "foo" for CREATE VIEW v AS SELECT foo.bar FROM foo;
  pub(crate) parent_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub(crate) struct ColumnMapping {
  pub(crate) columns: Vec<ViewColumn>,

  /// Group by that can be used as a key for record APIs.
  pub(crate) group_by: Option<usize>,

  /// A list of joins.
  pub(crate) joins: Vec<u8>,
}

fn extract_column_mapping(
  select: sqlite3_parser::ast::Select,
  tables: &[Table],
) -> Result<ColumnMapping, SchemaError> {
  let result_columns = extract_result_columns(&select)?;
  let group_by_key_candidate = extract_group_by_key_candidate(&select)?;

  let (joins, referenced_table_by_alias) =
    extract_joins_and_referenced_tables_by_alias(select, tables)?;

  let find_table_by_alias = |a: &str| -> Result<&ReferredTable, SchemaError> {
    return referenced_table_by_alias
      .iter()
      .find(|t| t.alias.as_ref().unwrap_or(&t.table.name.name) == a)
      .ok_or_else(|| precondition(&format!("Table '{a}' not found")));
  };

  // Search table in refernce order. SQLite checks comprehensively and will return an `ambiguous
  // column name: <col>` error at query time (as opposed to VIEW-creation-time).
  let find_column_by_unqualified_name =
    |colname: &str| -> Result<(&ReferredTable, &Column), SchemaError> {
      let mut found: Option<(&ReferredTable, &Column)> = None;
      for reft in &referenced_table_by_alias {
        if let Some(c) = reft.table.columns.iter().find(|c| c.name == colname)
          && found.replace((reft, c)).is_some()
        {
          return Err(precondition(&format!("Ambiguous column: {colname}")));
        }
      }
      return found.ok_or(precondition(&format!("Column '{colname}' not found")));
    };

  let mut mapping: Vec<ViewColumn> = vec![];
  for col in result_columns {
    match col {
      ResultColumn::Star => {
        for referred_table in &referenced_table_by_alias {
          for c in &referred_table.table.columns {
            mapping.push(ViewColumn {
              column: c.clone(),
              parent_name: get_parent_name(referred_table),
            });
          }
        }
      }
      ResultColumn::TableStar(name) => {
        let name = unquote_name(name);
        let referred_table = find_table_by_alias(&name)?;

        for c in &referred_table.table.columns {
          mapping.push(ViewColumn {
            column: c.clone(),
            parent_name: get_parent_name(referred_table),
          });
        }
      }
      ResultColumn::Expr(expr, alias) => match expr {
        Expr::Id(id) => {
          let (referred_table, column) = find_column_by_unqualified_name(&unquote_id(id.clone()))?;

          mapping.push(ViewColumn {
            column: column_with_alias(
              &column,
              to_alias(alias).unwrap_or_else(|| column.name.clone()),
            ),
            parent_name: get_parent_name(referred_table),
          });
        }
        Expr::Qualified(qualifier, name) => {
          let qualifier = unquote_name(qualifier);
          let referred_table = find_table_by_alias(&qualifier)?;

          let col_name = unquote_name(name);
          let column = referred_table
            .table
            .columns
            .iter()
            .find(|c| c.name == col_name)
            .ok_or_else(|| precondition(&format!("Column '{col_name}' not found")))?;

          mapping.push(ViewColumn {
            column: column_with_alias(
              column,
              to_alias(alias).unwrap_or_else(|| column.name.clone()),
            ),
            parent_name: get_parent_name(referred_table),
          });
        }
        Expr::Cast { expr, type_name } => {
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

          mapping.push(ViewColumn {
            column: Column {
              // NOTE: the sqlite3 CLI also simply uses the entire expr.
              name: to_alias(alias).unwrap_or_else(|| expr.to_string()),
              type_name: type_name.name,
              data_type,
              affinity_type: ColumnAffinityType::from_data_type(data_type),
              options: vec![],
            },
            parent_name: None,
          });
        }
        expr => {
          // Handle type-inference of some built-in functions for convenience to reduce the need
          // for explicit CAST(expr AS type), e.g. `MAX(column)`.
          if let Expr::FunctionCall { name, args, .. } = expr
            && builtin_function_preserving_type(name)
          {
            match extract_single_arg(args) {
              Some((Some(qualifier), name)) => {
                let referred_table = find_table_by_alias(&qualifier)?;
                let column = referred_table
                  .table
                  .columns
                  .iter()
                  .find(|c| c.name == name)
                  .ok_or_else(|| precondition(&format!("Column '{name}' not found")))?;

                mapping.push(ViewColumn {
                  column: column_with_alias(
                    column,
                    to_alias(alias).unwrap_or_else(|| column.name.to_string()),
                  ),
                  parent_name: get_parent_name(referred_table),
                });

                continue;
              }
              Some((None, name)) => {
                let (referred_table, column) = find_column_by_unqualified_name(&name)?;

                mapping.push(ViewColumn {
                  column: column_with_alias(
                    column,
                    to_alias(alias).unwrap_or_else(|| column.name.to_string()),
                  ),
                  parent_name: get_parent_name(referred_table),
                });

                continue;
              }
              _ => {}
            }
          }

          // We cannot map arbitrary expressions.
          return Err(precondition("Unsupported expr, cannot derive type"));
        }
      },
    };
  }

  return match group_by_key_candidate {
    None => Ok(ColumnMapping {
      columns: mapping,
      group_by: None,
      joins,
    }),
    Some(group_by) => {
      // NOTE: GROUP BY can technically reference any column, but only columns also exposed by the
      // VIEW are useful to us as keys. In other words, there's no point of us to search for this
      // column through all referenced tables.
      let group_by = match group_by.qualifier {
        Some(ref qualifier) => {
          // If the "GROUP BY" uses a qualifier, it must reference a table or subselect, e.g.:
          //   CREATE VIEW v AS SELECT a.id FROM a RIGHT JOIN b ON a.id = b.id GROUP BY a.id;
          mapping.iter().position(|v: &ViewColumn| {
            v.parent_name.as_ref() == Some(qualifier) && v.column.name == group_by.name
          })
        }
        None => mapping
          .iter()
          .position(|v: &ViewColumn| v.column.name == group_by.name),
      };

      Ok(ColumnMapping {
        columns: mapping,
        group_by: Some(group_by.ok_or_else(|| precondition("GROUP BY column not exposed"))?),
        joins,
      })
    }
  };
}

fn get_parent_name(referred_table: &ReferredTable) -> Option<String> {
  return Some(
    referred_table
      .alias
      .as_ref()
      .unwrap_or(&referred_table.table.name.name)
      .to_owned(),
  );
}

#[derive(Clone, Debug)]
pub(crate) struct ReferredTable {
  /// Optional top-most alias (nested aliases, e.g. in a sub-query,  are not accessible).
  pub(crate) alias: Option<String>,

  /// The referenced table.
  pub(crate) table: Table,
}

fn extract_joins_and_referenced_tables_by_alias(
  select: sqlite3_parser::ast::Select,
  tables: &[Table],
) -> Result<(Vec<u8>, Vec<ReferredTable>), SchemaError> {
  let body = select.body;
  if body.compounds.is_some() {
    return Err(precondition("Compound queries not supported"));
  }

  let sqlite3_parser::ast::OneSelect::Select {
    columns: _,
    distinctness,
    from,
    group_by: _,
    having: _,
    where_clause: _,
    window_clause,
  } = body.select
  else {
    return Err(precondition(&format!(
      "VALUES not supported: {:?}",
      body.select
    )));
  };

  if distinctness.is_some() {
    return Err(precondition("DISTINCT clause not (yet) supported"));
  }

  if window_clause.is_some() {
    return Err(precondition("WINDOW clause not (yet) supported"));
  }

  // First build list of referenced tables and their aliases.
  let Some(FromClause {
    select: nested_select,
    joins,
    ..
  }) = from
  else {
    return Err(precondition("missing FROM clause"));
  };

  let find_table = |qualified_name: &QualifiedName| -> Result<&Table, SchemaError> {
    let Some(table) = tables.iter().find(|t| t.name == *qualified_name) else {
      return Err(precondition(&format!("Missing table: {qualified_name:?}")));
    };

    // Make sure all referenced tables are strict.
    if !table.strict {
      return Err(precondition(&format!(
        "Table {:?} must be STRICT to derive type",
        table.name
      )));
    }

    return Ok(table);
  };

  let mut all_joins: Vec<u8> = joins
    .as_ref()
    .map(|joins| {
      joins
        .iter()
        .map(|join| extract_join_type(join.operator).bits())
        .collect()
    })
    .unwrap_or_default();

  // List of referenced tables in insertion order (left-to-right).
  let referenced_table_by_alias: Vec<ReferredTable> = match nested_select.map(|s| *s) {
    Some(SelectTable::Table(fqn, alias, _indexed)) => {
      // Table itself
      let mut referenced_tables = vec![ReferredTable {
        alias: to_alias(alias),
        table: find_table(&fqn.into())?.clone(),
      }];

      // // Plus possible joins.
      for join in joins.unwrap_or_default() {
        // We don't currently allow joining sub-queries, etc.
        match join.table {
          SelectTable::Table(fqn, alias, _indexed) => {
            referenced_tables.push(ReferredTable {
              alias: to_alias(alias),
              table: find_table(&fqn.into())?.clone(),
            });
          }
          SelectTable::Select(subselect, alias) => {
            let alias = to_alias(alias);

            let (joins_in_subselect, referenced_tables_in_subselect) =
              extract_joins_and_referenced_tables_by_alias(*subselect, tables)?;

            all_joins.extend(joins_in_subselect);
            referenced_tables.extend(referenced_tables_in_subselect.into_iter().map(
              |ReferredTable { table, .. }| -> ReferredTable {
                return ReferredTable {
                  alias: alias.clone(),
                  table,
                };
              },
            ));
          }
          _ => {
            return Err(precondition("JOIN with TABLE expected"));
          }
        };
      }

      referenced_tables
    }
    Some(SelectTable::Select(nested_select, alias)) => {
      // Simply recurse tu unnest the select.
      let alias = to_alias(alias);
      let (joins_in_nested_select, referenced_tables_in_nested_select) =
        extract_joins_and_referenced_tables_by_alias(*nested_select, tables)?;

      return Ok((
        joins_in_nested_select,
        referenced_tables_in_nested_select
          .into_iter()
          .map(|referred_table| ReferredTable {
            // NOTE: Reset the alias.
            alias: alias.clone(),
            table: referred_table.table,
          })
          .collect(),
      ));
    }
    Some(x) => {
      return Err(precondition(&format!(
        "The following sub-query is not (yet) supported: {x:?}"
      )));
    }
    None => {
      return Err(precondition("missing SELECT"));
    }
  };

  return Ok((all_joins, referenced_table_by_alias));
}

#[inline]
fn precondition(m: &str) -> SchemaError {
  return SchemaError::Precondition(m.into());
}

fn extract_single_arg(args: Option<Vec<Expr>>) -> Option<(Option<String>, String)> {
  return match args {
    Some(mut args) if args.len() == 1 => match args.remove(0) {
      Expr::Id(id) => Some((None, unquote_id(id))),
      Expr::Qualified(qualifier, name) => Some((Some(unquote_name(qualifier)), unquote_name(name))),
      _ => None,
    },
    _ => None,
  };
}

fn extract_join_type(op: JoinOperator) -> JoinType {
  return match op {
    JoinOperator::TypedJoin(Some(t)) => t,
    JoinOperator::Comma | JoinOperator::TypedJoin(None) => JoinType::INNER,
  };
}

fn extract_result_columns(
  select: &sqlite3_parser::ast::Select,
) -> Result<Vec<ResultColumn>, SchemaError> {
  let sqlite3_parser::ast::OneSelect::Select { columns, .. } = &select.body.select else {
    return Err(precondition("VALUES not supported"));
  };
  return Ok(columns.clone());
}

struct GroupBy {
  qualifier: Option<String>,
  name: String,
}

fn extract_group_by_key_candidate(
  select: &sqlite3_parser::ast::Select,
) -> Result<Option<GroupBy>, SchemaError> {
  let sqlite3_parser::ast::OneSelect::Select { group_by, .. } = &select.body.select else {
    return Err(precondition("VALUES not supported"));
  };

  let Some(group_by) = group_by else {
    return Ok(None);
  };

  return match group_by.len() {
    1 => match group_by[0].clone() {
      Expr::Id(id) => Ok(Some(GroupBy {
        qualifier: None,
        name: unquote_id(id),
      })),
      Expr::Name(name) => Ok(Some(GroupBy {
        qualifier: None,
        name: unquote_name(name),
      })),
      Expr::Qualified(qualifier, name) => Ok(Some(GroupBy {
        qualifier: Some(unquote_name(qualifier)),
        name: unquote_name(name),
      })),
      expr => Err(precondition(&format!(
        "For RecordAPIs GROUP BY expressions must reference an exposed VIEW column, got {expr:?}"
      ))),
    },
    n => Err(precondition(&format!(
      "For RecordAPIs GROUP BY expressions must reference a single VIEW column, got {n}"
    ))),
  };
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
fn unquote_string(s: &str) -> String {
  let n = s.as_bytes();
  if n.is_empty() {
    return String::new();
  }

  return match n[0] {
    b'"' | b'`' | b'\'' | b'[' => {
      assert!(n.len() >= 2, "string: {s}");
      s[1..n.len() - 1].to_string()
    }
    _ => s.to_string(),
  };
}

fn unquote_name(name: Name) -> String {
  return unquote_string(&name.0);
}

fn unquote_qualified(name: AstQualifiedName) -> String {
  return unquote_name(name.name);
}

fn unquote_db_name(name: &AstQualifiedName) -> Option<String> {
  return name.db_name.clone().map(unquote_name);
}

fn unquote_id(id: sqlite3_parser::ast::Id) -> String {
  return unquote_string(&id.0);
}

fn unquote_expr(expr: Expr) -> String {
  return match expr {
    Expr::Name(n) => unquote_name(n),
    Expr::Id(id) => unquote_id(id),
    Expr::Literal(Literal::String(s)) => unquote_string(&s),
    x => x.to_string(),
  };
}

fn to_alias(alias: Option<sqlite3_parser::ast::As>) -> Option<String> {
  return alias.map(|a| match a {
    // "FROM table_name AS alias"
    sqlite3_parser::ast::As::As(name) => unquote_name(name),
    // "FROM table_name alias"
    sqlite3_parser::ast::As::Elided(name) => unquote_name(name),
  });
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

  let Some(stmt) = crate::parse::parse_into_statement(&sql)? else {
    anyhow::bail!("Not a statement");
  };

  return Ok(stmt.try_into()?);
}

fn builtin_function_preserving_type(name: sqlite3_parser::ast::Id) -> bool {
  let name = unquote_id(name).to_uppercase();
  return matches!(name.as_str(), "MAX" | "MIN" | "SUM");
}

fn column_with_alias(column: &Column, alias: String) -> Column {
  return Column {
    name: alias,
    type_name: column.type_name.clone(),
    data_type: column.data_type,
    affinity_type: column.affinity_type,
    options: column.options.clone(),
  };
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::parse::parse_into_statement;

  #[test]
  fn test_quote() {
    assert_eq!("", quote(&vec![]));
    assert_eq!("''", quote(&vec!["".to_string()]));
    assert_eq!("'foo', ''", quote(&vec!["foo".to_string(), "".to_string()]));
  }

  #[test]
  fn test_unquote() {
    assert_eq!(unquote_name(Name("".into())), "");
    assert_eq!(unquote_name(Name("['``']".into())), "'``'");
    assert_eq!(unquote_name(Name("\"[]\"".into())), "[]");
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

    let parsed = parse_into_statement(&statement).unwrap().unwrap();

    let table: Table = parsed.try_into().unwrap();
    assert_eq!(table.name, table_name);
    let sql = table.create_table_statement();

    assert_eq!(
      "CREATE TABLE \"table\" ('index' TEXT, 'delete' TEXT, 'create' TEXT) STRICT",
      sql
    );
    parse_into_statement(&sql).unwrap().unwrap();
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
      let conn = trailbase_extension::connect_sqlite(None).unwrap();
      conn.execute(&statement, ()).unwrap();
    }

    let statement1 = parse_into_statement(&statement).unwrap().unwrap();
    let table1: Table = statement1.clone().try_into().unwrap();

    let sql = table1.create_table_statement();
    {
      // Same as above, make sure the constructed query is valid as opposed to "only" parsable.
      let conn = trailbase_extension::connect_sqlite(None).unwrap();
      conn.execute(&sql, ()).unwrap();
    }

    let statement2 = parse_into_statement(&sql).unwrap().unwrap();

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

    let statement1 = parse_into_statement(SQL).unwrap().unwrap();
    let index1: TableIndex = statement1.clone().try_into().unwrap();

    let statement2 = parse_into_statement(&index1.create_index_statement())
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

    parse_into_statement(SQL).unwrap().unwrap();
  }

  #[test]
  fn test_parse_create_index() {
    let sql =
      r#"CREATE UNIQUE INDEX "main"."index_name" ON 'table_name' (a ASC, b DESC) WHERE x > 0"#;
    let index: TableIndex = parse_into_statement(sql)
      .unwrap()
      .unwrap()
      .try_into()
      .unwrap();

    let sql1 = index.create_index_statement();
    let stmt1 = parse_into_statement(&sql1).unwrap().unwrap();
    let index1: TableIndex = stmt1.try_into().unwrap();

    assert_eq!(index, index1, "Parsed: {sql1}");
  }

  fn parse_into_select(sql: &str) -> sqlite3_parser::ast::Select {
    let sqlite3_parser::ast::Stmt::Select(select) = parse_into_statement(sql).unwrap().unwrap()
    else {
      panic!("Not a select");
    };
    return *select;
  }

  #[test]
  fn test_view_column_extraction() {
    let tables = vec![Table {
      name: QualifiedName {
        name: "table_name".to_string(),
        database_schema: None,
      },
      strict: true,
      columns: vec![Column {
        name: "column".to_string(),
        type_name: "teXt".to_string(),
        data_type: ColumnDataType::Text,
        affinity_type: ColumnAffinityType::Text,
        options: vec![],
      }],
      foreign_keys: vec![],
      unique: vec![],
      checks: vec![],
      virtual_table: false,
      temporary: false,
    }];

    {
      // No alias
      let select = parse_into_select("SELECT column FROM table_name");
      let _mapping = extract_column_mapping(select, &tables).unwrap();
    }

    {
      // With alias
      let select = parse_into_select("SELECT alias.column FROM table_name AS alias");
      let _mapping = extract_column_mapping(select, &tables).unwrap();
    }

    {
      // With "elided" alias
      let select = parse_into_select("SELECT alias.column FROM table_name alias");
      let _mapping = extract_column_mapping(select, &tables).unwrap();
    }

    {
      // JOIN on a SELECT.
      let select = parse_into_select(
        "SELECT x.column, y.column AS foo FROM table_name AS x LEFT JOIN (SELECT * FROM table_name) AS y ON x.column = y.column",
      );
      let column_mapping = extract_column_mapping(select, &tables).unwrap();
      let columns = &column_mapping.columns;
      assert_eq!(columns.len(), 2, "{columns:?}");

      let first = &columns[0];
      assert_eq!(first.column.data_type, ColumnDataType::Text);
      assert_eq!(first.column.name, "column");

      let second = &columns[1];
      assert_eq!(second.column.data_type, ColumnDataType::Text);
      assert_eq!(second.column.name, "foo");
    }

    {
      // Compound SELECT.
      let select =
        parse_into_select("SELECT column FROM table_name UNION SELECT column FROM table_name");
      let err = extract_column_mapping(select, &tables)
        .err()
        .unwrap()
        .to_string();
      assert!(err.contains("Compound queries not supported"), "{err}");
    }
  }

  fn parse_create_view_select(sql: &str) -> sqlite3_parser::ast::Select {
    let sqlite3_parser::ast::Stmt::CreateView { select, .. } =
      parse_into_statement(sql).unwrap().unwrap()
    else {
      panic!("Not a CREATE VIEW: {sql}");
    };
    return *select;
  }

  #[test]
  fn test_creare_view_colum_mapping() {
    let table_a = parse_create_table(
      "CREATE TABLE a (id INTEGER PRIMARY KEY, data TEXT NOT NULL DEFAULT '') STRICT",
    );

    let tables = [table_a];

    let select =
      parse_create_view_select("CREATE VIEW view0 AS SELECT x.id FROM a AS x GROUP BY x.id");
    assert_eq!(
      Some(0),
      extract_column_mapping(select, &tables).unwrap().group_by
    );

    let select =
      parse_create_view_select("CREATE VIEW view0 AS SELECT x.id FROM a AS x GROUP BY id");
    assert_eq!(
      Some(0),
      extract_column_mapping(select, &tables).unwrap().group_by
    )
  }

  fn parse_create_table(create_table_sql: &str) -> Table {
    let create_table_statement = parse_into_statement(create_table_sql).unwrap().unwrap();
    return create_table_statement.try_into().unwrap();
  }

  #[test]
  fn test_view_column_extraction_join() {
    let profiles_table = parse_create_table(
      r#"
        CREATE TABLE bar.profiles (
            user             BLOB PRIMARY KEY NOT NULL REFERENCES _user(id),
            username         TEXT NOT NULL
        ) STRICT;
      "#,
    );

    let articles_table = parse_create_table(
      r#"
        CREATE TABLE foo.articles (
            id               BLOB PRIMARY KEY NOT NULL,
            author           BLOB NOT NULL REFERENCES _user(id),
            body             TEXT
        ) STRICT;
      "#,
    );

    let tables = [profiles_table, articles_table];

    let select = parse_into_select(
      "SELECT user, *, a.*, p.user AS foo FROM foo.articles AS a LEFT JOIN bar.profiles AS p ON p.user = a.author",
    );
    let mapping = extract_column_mapping(select, &tables).unwrap();

    assert_eq!(
      mapping
        .columns
        .iter()
        .map(|m| m.column.name.as_str())
        .collect::<Vec<_>>(),
      [
        "user", "id", "author", "body", "user", "username", "id", "author", "body", "foo"
      ]
    );
  }
}
