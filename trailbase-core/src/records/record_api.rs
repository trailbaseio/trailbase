use askama::Template;
use log::*;
use rusqlite::types::ToSqlOutput;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use trailbase_schema::metadata::{
  find_file_column_indexes, find_user_id_foreign_key_columns, JsonColumnMetadata, TableMetadata,
  TableOrViewMetadata, ViewMetadata,
};
use trailbase_schema::sqlite::{sqlite3_parse_into_statement, Column, ColumnDataType};
use trailbase_sqlite::{NamedParamRef, NamedParams, Params as _, Value};

use crate::auth::user::User;
use crate::config::proto::{ConflictResolutionStrategy, RecordApiConfig};
use crate::constants::USER_TABLE;
use crate::records::params::{prefix_colon, LazyParams};
use crate::records::{Permission, RecordError};
use crate::util::{assert_uuidv7, b64_to_id};

#[derive(Clone)]
pub struct RecordApi {
  state: Arc<RecordApiState>,
}

struct RecordApiSchema {
  /// Schema metadata
  table_name: String,
  is_table: bool,
  record_pk_column: (usize, Column),
  columns: Vec<Column>,
  json_column_metadata: Vec<Option<JsonColumnMetadata>>,
  has_file_columns: bool,
  user_id_columns: Vec<usize>,

  // Helpers
  name_to_index: HashMap<String, usize>,
  named_params_template: NamedParams,
}

impl RecordApiSchema {
  fn from_table(table_metadata: &TableMetadata, config: &RecordApiConfig) -> Result<Self, String> {
    assert_eq!(config.table_name.as_deref(), Some(table_metadata.name()));

    let Some((pk_index, pk_column)) = table_metadata.record_pk_column() else {
      return Err("RecordApi requires integer/UUIDv7 primary key column".into());
    };
    let record_pk_column = (pk_index, pk_column.clone());

    let (columns, json_column_metadata) = filter_columns(
      config,
      &table_metadata.schema.columns,
      &table_metadata.json_metadata.columns,
    );

    let has_file_columns = !find_file_column_indexes(&json_column_metadata).is_empty();
    let user_id_columns = find_user_id_foreign_key_columns(&columns, USER_TABLE);

    let name_to_index = HashMap::<String, usize>::from_iter(
      columns
        .iter()
        .enumerate()
        .map(|(index, col)| (col.name.clone(), index)),
    );

    let named_params_template: NamedParams = columns
      .iter()
      .map(|column| {
        (
          Cow::Owned(prefix_colon(&column.name)),
          trailbase_sqlite::Value::Null,
        )
      })
      .collect();

    return Ok(Self {
      table_name: table_metadata.name().to_string(),
      is_table: true,
      record_pk_column,
      columns,
      json_column_metadata,
      has_file_columns,
      user_id_columns,
      name_to_index,
      named_params_template,
    });
  }

  pub fn from_view(view_metadata: &ViewMetadata, config: &RecordApiConfig) -> Result<Self, String> {
    assert_eq!(config.table_name.as_deref(), Some(view_metadata.name()));

    let Some((pk_index, pk_column)) = view_metadata.record_pk_column() else {
      return Err(format!(
        "RecordApi requires integer/UUIDv7 primary key column: {config:?}"
      ));
    };
    let record_pk_column = (pk_index, pk_column.clone());

    let Some(ref columns) = view_metadata.schema.columns else {
      return Err("RecordApi requires schema".to_string());
    };
    let Some(json_metadata) = view_metadata.json_metadata() else {
      return Err("RecordApi requires json metadata".to_string());
    };

    let (columns, json_column_metadata) = filter_columns(config, columns, &json_metadata.columns);

    let has_file_columns = !find_file_column_indexes(&json_column_metadata).is_empty();
    let user_id_columns = find_user_id_foreign_key_columns(&columns, USER_TABLE);

    let name_to_index = HashMap::<String, usize>::from_iter(
      columns
        .iter()
        .enumerate()
        .map(|(index, col)| (col.name.clone(), index)),
    );

    return Ok(Self {
      table_name: view_metadata.name().to_string(),
      is_table: false,
      record_pk_column,
      columns,
      json_column_metadata,
      has_file_columns,
      user_id_columns,
      name_to_index,
      named_params_template: NamedParams::new(),
    });
  }
}

struct RecordApiState {
  /// Database connection for access checks.
  conn: trailbase_sqlite::Connection,

  /// Schema metadata
  schema: RecordApiSchema,

  // Below properties are filled from `proto::RecordApiConfig`.
  api_name: String,
  acl: [u8; 2],
  insert_conflict_resolution_strategy: Option<ConflictResolutionStrategy>,
  insert_autofill_missing_user_id_columns: bool,
  enable_subscriptions: bool,

  // Foreign key expansion configuration. Affects schema.
  expand: Option<HashMap<String, serde_json::Value>>,

  // Open question: right now the read_access rule is also used for listing. It might be nice to
  // allow different permissions, however there's a risk of listing records w/o read access.
  // Arguably, this could always be modeled as two APIs with different permissions on the same
  // table.
  read_access_rule: Option<String>,
  read_access_query: Option<String>,
  subscription_read_access_query: Option<String>,

  create_access_query: Option<String>,
  update_access_query: Option<String>,
  delete_access_query: Option<String>,
  schema_access_query: Option<String>,
}

impl RecordApiState {
  #[inline]
  fn cached_access_query(&self, p: Permission) -> Option<&str> {
    return match p {
      Permission::Create => self.create_access_query.as_deref(),
      Permission::Read => self.read_access_query.as_deref(),
      Permission::Update => self.update_access_query.as_deref(),
      Permission::Delete => self.delete_access_query.as_deref(),
      Permission::Schema => self.schema_access_query.as_deref(),
    };
  }
}

impl RecordApi {
  pub fn from_table(
    conn: trailbase_sqlite::Connection,
    table_metadata: &TableMetadata,
    config: RecordApiConfig,
  ) -> Result<Self, String> {
    assert_eq!(config.table_name.as_deref(), Some(table_metadata.name()));

    let schema = RecordApiSchema::from_table(table_metadata, &config)?;

    return Self::from_impl(conn, schema, config);
  }

  pub fn from_view(
    conn: trailbase_sqlite::Connection,
    view_metadata: &ViewMetadata,
    config: RecordApiConfig,
  ) -> Result<Self, String> {
    assert_eq!(config.table_name.as_deref(), Some(view_metadata.name()));

    let schema = RecordApiSchema::from_view(view_metadata, &config)?;

    return Self::from_impl(conn, schema, config);
  }

  fn from_impl(
    conn: trailbase_sqlite::Connection,
    schema: RecordApiSchema,
    config: RecordApiConfig,
  ) -> Result<Self, String> {
    assert_eq!(schema.columns.len(), schema.json_column_metadata.len());

    let Some(api_name) = config.name.clone() else {
      return Err(format!("RecordApi misses name: {config:?}"));
    };

    let (read_access_query, subscription_read_access_query) = match &config.read_access_rule {
      Some(rule) => {
        let read_access_query =
          build_read_delete_schema_query(&schema.table_name, &schema.record_pk_column.1.name, rule);

        let subscription_read_access_query = if schema.is_table {
          Some(
            SubscriptionRecordReadTemplate {
              read_access_rule: rule,
              column_names: schema.columns.iter().map(|c| c.name.as_str()).collect(),
            }
            .render()
            .map_err(|err| err.to_string())?,
          )
        } else {
          None
        };

        (Some(read_access_query), subscription_read_access_query)
      }
      None => (None, None),
    };

    let delete_access_query = config.delete_access_rule.as_ref().map(|rule| {
      build_read_delete_schema_query(&schema.table_name, &schema.record_pk_column.1.name, rule)
    });

    let schema_access_query = config.schema_access_rule.as_ref().map(|rule| {
      build_read_delete_schema_query(&schema.table_name, &schema.record_pk_column.1.name, rule)
    });

    let create_access_query = match &config.create_access_rule {
      Some(rule) => {
        if schema.is_table {
          Some(build_create_access_query(&schema.columns, rule)?)
        } else {
          None
        }
      }
      None => None,
    };

    let update_access_query = match &config.update_access_rule {
      Some(rule) => {
        if schema.is_table {
          Some(build_update_access_query(
            &schema.table_name,
            &schema.columns,
            &schema.record_pk_column.1.name,
            rule,
          )?)
        } else {
          None
        }
      }
      None => None,
    };

    return Ok(RecordApi {
      state: Arc::new(RecordApiState {
        conn,
        schema,

        // proto::RecordApiConfig properties below:
        api_name,

        // Insert- specific options.
        insert_conflict_resolution_strategy: config
          .conflict_resolution
          .and_then(|cr| cr.try_into().ok()),
        insert_autofill_missing_user_id_columns: config
          .autofill_missing_user_id_columns
          .unwrap_or(false),
        enable_subscriptions: config.enable_subscriptions.unwrap_or(false),

        expand: if config.expand.is_empty() {
          None
        } else {
          Some(
            config
              .expand
              .iter()
              .map(|col_name| (col_name.to_string(), serde_json::Value::Null))
              .collect(),
          )
        },

        // Access control lists.
        acl: [
          convert_acl(&config.acl_world),
          convert_acl(&config.acl_authenticated),
        ],
        // Access rules.
        //
        // Create:

        // The raw read rule is needed to construct list queries.
        read_access_rule: config.read_access_rule,
        read_access_query,
        subscription_read_access_query,

        create_access_query,
        update_access_query,
        delete_access_query,
        schema_access_query,
      }),
    });
  }

  #[inline]
  pub fn api_name(&self) -> &str {
    &self.state.api_name
  }

  #[inline]
  pub fn table_name(&self) -> &str {
    return &self.state.schema.table_name;
  }

  #[inline]
  pub fn has_file_columns(&self) -> bool {
    return self.state.schema.has_file_columns;
  }

  #[inline]
  pub fn user_id_columns(&self) -> &[usize] {
    return &self.state.schema.user_id_columns;
  }

  #[inline]
  pub(crate) fn expand(&self) -> Option<&HashMap<String, serde_json::Value>> {
    return self.state.expand.as_ref();
  }

  #[inline]
  pub fn record_pk_column(&self) -> &(usize, Column) {
    return &self.state.schema.record_pk_column;
  }

  #[inline]
  pub fn columns(&self) -> &[Column] {
    return &self.state.schema.columns;
  }

  #[inline]
  pub fn json_column_metadata(&self) -> &[Option<JsonColumnMetadata>] {
    return &self.state.schema.json_column_metadata;
  }

  #[inline]
  pub fn is_table(&self) -> bool {
    return self.state.schema.is_table;
  }

  #[inline]
  pub fn column_index_by_name(&self, key: &str) -> Option<usize> {
    return self.state.schema.name_to_index.get(key).copied();
  }

  pub fn id_to_sql(&self, id: &str) -> Result<Value, RecordError> {
    return match self.state.schema.record_pk_column.1.data_type {
      ColumnDataType::Blob => {
        // Special handling for text encoded UUIDs. Right now we're guessing based on length, it
        // would be more explicit rely on CHECK(...) column options.
        if id.len() == 36 {
          if let Ok(id) = uuid::Uuid::parse_str(id) {
            return Ok(Value::Blob(id.into()));
          }
        }

        let record_id = b64_to_id(id).map_err(|_err| RecordError::BadRequest("Invalid id"))?;
        assert_uuidv7(&record_id);
        Ok(Value::Blob(record_id.into()))
      }
      ColumnDataType::Integer => Ok(Value::Integer(
        id.parse::<i64>()
          .map_err(|_err| RecordError::BadRequest("Invalid id"))?,
      )),
      _ => Err(RecordError::BadRequest("Invalid id")),
    };
  }

  #[inline]
  pub fn read_access_rule(&self) -> Option<&str> {
    return self.state.read_access_rule.as_deref();
  }

  #[inline]
  pub fn insert_autofill_missing_user_id_columns(&self) -> bool {
    return self.state.insert_autofill_missing_user_id_columns;
  }

  #[inline]
  pub fn enable_subscriptions(&self) -> bool {
    return self.state.enable_subscriptions;
  }

  #[inline]
  pub fn insert_conflict_resolution_strategy(&self) -> Option<ConflictResolutionStrategy> {
    return self.state.insert_conflict_resolution_strategy;
  }

  /// Check if the given user (if any) can access a record given the request and the operation.
  pub async fn check_record_level_access(
    &self,
    p: Permission,
    record_id: Option<&Value>,
    request_params: Option<&mut LazyParams<'_, RecordApi>>,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    // First check table level access and if present check row-level access based on access rule.
    self.check_table_level_access(p, user)?;

    let Some(access_query) = self.state.cached_access_query(p) else {
      return Ok(());
    };

    let params = self.build_named_params(p, record_id, request_params, user)?;
    // let state = self.state.clone();
    let access_query = access_query.to_string();

    match self
      .state
      .conn
      .call(move |conn| {
        // let access_query = state.cached_access_query(p).unwrap();
        let mut stmt = conn.prepare_cached(&access_query)?;
        params.bind(&mut stmt)?;

        let mut rows = stmt.raw_query();
        if let Some(row) = rows.next()? {
          return Ok(row.get(0)?);
        }

        return Err(rusqlite::Error::QueryReturnedNoRows.into());
      })
      .await
    {
      Ok(allowed) => {
        if allowed {
          return Ok(());
        }
      }
      Err(err) => {
        warn!("RLA query failed: {err}");

        #[cfg(test)]
        panic!("RLA query failed: {err}");
      }
    };

    return Err(RecordError::Forbidden);
  }

  /// Check if the given user (if any) can access a record given the request and the operation.
  ///
  /// NOTE: We could inline this in `SubscriptionManager::broker_subscriptions` and reduce some
  /// redundant work over sql parameter construction.
  #[inline]
  pub(crate) fn check_record_level_read_access_for_subscriptions(
    &self,
    conn: &rusqlite::Connection,
    record: &[(&str, rusqlite::types::ValueRef<'_>)],
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    // First check table level access and if present check row-level access based on access rule.
    self.check_table_level_access(Permission::Read, user)?;

    let Some(ref access_query) = self.state.subscription_read_access_query else {
      return Ok(());
    };

    let params = {
      let mut params = Vec::<NamedParamRef<'_>>::with_capacity(record.len() + 1);
      params.push((
        Cow::Borrowed(":__user_id"),
        user.map_or_else(
          || ToSqlOutput::Owned(Value::Null),
          |u| ToSqlOutput::Owned(Value::Blob(u.uuid.into())),
        ),
      ));

      params.extend(record.iter().map(|(name, value)| {
        (
          Cow::Owned(prefix_colon(name)),
          ToSqlOutput::Borrowed(*value),
        )
      }));

      params
    };

    let mut stmt = conn
      .prepare_cached(access_query)
      .map_err(|_err| RecordError::Forbidden)?;
    params
      .bind(&mut stmt)
      .map_err(|_err| RecordError::Forbidden)?;

    match stmt.raw_query().next() {
      Ok(Some(row)) => {
        if row.get(0).unwrap_or(false) {
          return Ok(());
        }
      }
      Ok(None) => {}
      Err(err) => {
        warn!("RLA query failed: {err}");

        #[cfg(test)]
        panic!("RLA query failed: {err}");
      }
    }

    return Err(RecordError::Forbidden);
  }

  #[inline]
  pub fn check_table_level_access(
    &self,
    p: Permission,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    if (user.is_some() && self.has_access(Entity::Authenticated, p))
      || self.has_access(Entity::World, p)
    {
      return Ok(());
    }

    return Err(RecordError::Forbidden);
  }

  #[inline]
  fn has_access(&self, e: Entity, p: Permission) -> bool {
    return (self.state.acl[e as usize] & (p as u8)) > 0;
  }

  // TODO: We should probably break this up into separate functions for CRUD, to only do and inject
  // what's actually needed. Maybe even break up the entire check_access_and_rls_then. It's pretty
  // winding right now.
  fn build_named_params(
    &self,
    p: Permission,
    record_id: Option<&Value>,
    request_params: Option<&mut LazyParams<'_, RecordApi>>,
    user: Option<&User>,
  ) -> Result<NamedParams, RecordError> {
    // We need to inject context like: record id, user, request, and row into the access
    // check. Below we're building the query and binding the context as params accordingly.
    let mut params = match p {
      Permission::Create | Permission::Update => {
        // Create and update cannot write to views.
        if !self.is_table() {
          return Err(RecordError::ApiRequiresTable);
        };

        let request_params = request_params
          .ok_or_else(|| RecordError::Internal("missing req params".into()))?
          .params()
          .map_err(|err| RecordError::Internal(err.into()))?;

        // NOTE: We cannot have access queries access missing _REQ_.props. So we need to inject an
        // explicit NULL value for all missing fields on the request. Can we make this cheaper,
        // either by pre-processing the access query or improving construction?
        let mut named_params = self.state.schema.named_params_template.clone();

        // 'outer: for (placeholder, value) in &request_params.named_params {
        //   for (p, ref mut v) in named_params.iter_mut() {
        //     if *placeholder == *p {
        //       *v = value.clone();
        //       continue 'outer;
        //     }
        //   }
        // }

        for (param_index, col_name) in request_params.column_names.iter().enumerate() {
          let Some(col_index) = self.column_index_by_name(col_name) else {
            // We simply skip unknown columns, this could simply be malformed input or version skew.
            // This is similar in spirit to protobuf's unknown fields behavior.
            continue;
          };

          named_params[col_index].1 = request_params.named_params[param_index].1.clone();
        }

        named_params
      }
      Permission::Read | Permission::Delete | Permission::Schema => NamedParams::with_capacity(2),
    };

    params.push((
      Cow::Borrowed(":__user_id"),
      user.map_or(Value::Null, |u| Value::Blob(u.uuid.into())),
    ));
    params.push((
      Cow::Borrowed(":__record_id"),
      record_id.map_or(Value::Null, |id| id.clone()),
    ));

    return Ok(params);
  }
}

pub(crate) fn validate_rule(rule: &str) -> Result<(), String> {
  let stmt = sqlite3_parse_into_statement(&format!("SELECT {rule}"))
    .map_err(|err| format!("'{rule}' not a valid SQL expression: {err}"))?;

  let Some(sqlite3_parser::ast::Stmt::Select(select)) = stmt else {
    panic!("Expected SELECT");
  };

  let sqlite3_parser::ast::OneSelect::Select { mut columns, .. } = select.body.select else {
    panic!("Expected SELECT");
  };

  if columns.len() != 1 {
    return Err("Expected single column".to_string());
  }

  let sqlite3_parser::ast::ResultColumn::Expr(expr, _) = columns.swap_remove(0) else {
    return Err("Expected expr".to_string());
  };

  validate_expr_recursively(&expr)?;

  return Ok(());
}

fn validate_expr_recursively(expr: &sqlite3_parser::ast::Expr) -> Result<(), String> {
  use sqlite3_parser::ast;

  match &expr {
    ast::Expr::Binary(lhs, _op, rhs) => {
      validate_expr_recursively(lhs)?;
      validate_expr_recursively(rhs)?;
    }
    ast::Expr::IsNull(inner) => {
      validate_expr_recursively(inner)?;
    }
    // ast::Expr::InTable { lhs, rhs, .. } => {
    //   match rhs {
    //     ast::QualifiedName {
    //       name: ast::Name(name),
    //       ..
    //     } if name == "_FIELDS_" => {
    //       if !matches!(**lhs, ast::Expr::Literal(ast::Literal::String(_))) {
    //         return Err(format!("Expected literal string: {lhs:?}"));
    //       }
    //     }
    //     _ => {}
    //   };
    //
    //   validate_expr_recursively(lhs)?;
    // }
    _ => {}
  }

  return Ok(());
}

#[derive(Template)]
#[template(
  escape = "none",
  whitespace = "minimize",
  path = "subscription_record_read.sql"
)]
struct SubscriptionRecordReadTemplate<'a> {
  read_access_rule: &'a str,
  column_names: Vec<&'a str>,
}

/// Build access query for record reads, deletes and query access.
///
/// Assumes access_rule is an expression: https://www.sqlite.org/syntax/expr.html
fn build_read_delete_schema_query(
  table_name: &str,
  pk_column_name: &str,
  access_rule: &str,
) -> String {
  return indoc::formatdoc!(
    r#"
      SELECT
        CAST(({access_rule}) AS INTEGER)
      FROM
        (SELECT :__user_id AS id) AS _USER_,
        (SELECT * FROM "{table_name}" WHERE "{pk_column_name}" = :__record_id) AS _ROW_
    "#
  );
}

#[derive(Template)]
#[template(
  escape = "none",
  whitespace = "minimize",
  path = "create_record_access_query.sql"
)]
struct CreateRecordAccessQueryTemplate<'a> {
  create_access_rule: &'a str,
  column_names: Vec<&'a str>,
}

/// Build access query for record creation.
///
/// Assumes access_rule is an expression: https://www.sqlite.org/syntax/expr.html
fn build_create_access_query(
  columns: &[Column],
  create_access_rule: &str,
) -> Result<String, String> {
  let column_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();

  return CreateRecordAccessQueryTemplate {
    create_access_rule,
    column_names,
  }
  .render()
  .map_err(|err| err.to_string());
}

#[derive(Template)]
#[template(
  escape = "none",
  whitespace = "minimize",
  path = "update_record_access_query.sql"
)]
struct UpdateRecordAccessQueryTemplate<'a> {
  update_access_rule: &'a str,
  table_name: &'a str,
  pk_column_name: &'a str,
  column_names: Vec<&'a str>,
}

/// Build access query for record updates.
///
/// Assumes access_rule is an expression: https://www.sqlite.org/syntax/expr.html
fn build_update_access_query(
  table_name: &str,
  columns: &[Column],
  pk_column_name: &str,
  update_access_rule: &str,
) -> Result<String, String> {
  let column_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();

  return UpdateRecordAccessQueryTemplate {
    update_access_rule,
    table_name,
    pk_column_name,
    column_names,
  }
  .render()
  .map_err(|err| err.to_string());
}

fn convert_acl(acl: &Vec<i32>) -> u8 {
  let mut value: u8 = 0;
  for flag in acl {
    value |= *flag as u8;
  }
  return value;
}

// Note: ACLs and entities are only enforced on the table-level, this owner (row-level concept) is
// not here.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Entity {
  World = 0,
  Authenticated = 1,
}

fn filter_columns(
  _config: &RecordApiConfig,
  columns: &[Column],
  json_column_metadata: &[Option<JsonColumnMetadata>],
) -> (Vec<Column>, Vec<Option<JsonColumnMetadata>>) {
  assert_eq!(columns.len(), json_column_metadata.len());
  return (columns.to_vec(), json_column_metadata.to_vec());
}

#[cfg(test)]
mod tests {
  use trailbase_schema::sqlite::sqlite3_parse_into_statement;

  use super::*;
  use crate::{config::proto::PermissionFlag, records::Permission};

  fn sanitize_template(template: &str) {
    assert!(sqlite3_parse_into_statement(template).is_ok(), "{template}");
    assert!(!template.contains("   "), "{template}");
    assert!(!template.contains("\n\n"), "{template}");
  }

  #[test]
  fn test_create_record_access_query_template() {
    {
      let query = CreateRecordAccessQueryTemplate {
        create_access_rule: "_USER_.id = X'05'",
        column_names: vec![],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = CreateRecordAccessQueryTemplate {
        create_access_rule: r#"_USER_.id = X'05' AND "index" = 'secret'"#,
        column_names: vec!["index"],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }
  }

  #[test]
  fn test_update_record_access_query_template() {
    {
      let query = UpdateRecordAccessQueryTemplate {
        update_access_rule: r#"_USER_.id = X'05' AND _ROW_."index" = 'secret'"#,
        table_name: "table",
        pk_column_name: "index",
        column_names: vec![],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = UpdateRecordAccessQueryTemplate {
        update_access_rule: r#"_USER_.id = X'05' AND _ROW_."index" = _REQ_."index""#,
        table_name: "table",
        pk_column_name: "index",
        column_names: vec!["index"],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }
  }

  #[test]
  fn test_subscription_record_read_template() {
    {
      let query = SubscriptionRecordReadTemplate {
        read_access_rule: "TRUE",
        column_names: vec![],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = SubscriptionRecordReadTemplate {
        read_access_rule: r#"_USER_.id = X'05' AND "index" = 'secret'"#,
        column_names: vec!["index"],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }
  }

  fn has_access(flags: u8, p: Permission) -> bool {
    return (flags & (p as u8)) > 0;
  }

  #[test]
  fn test_acl_conversion() {
    {
      let acl = convert_acl(&vec![PermissionFlag::Read as i32]);
      assert!(has_access(acl, Permission::Read));
    }

    {
      let acl = convert_acl(&vec![
        PermissionFlag::Read as i32,
        PermissionFlag::Create as i32,
      ]);
      assert!(has_access(acl, Permission::Read));
      assert!(has_access(acl, Permission::Create));
    }

    {
      let acl = convert_acl(&vec![
        PermissionFlag::Delete as i32,
        PermissionFlag::Update as i32,
      ]);
      assert!(has_access(acl, Permission::Delete));
      assert!(has_access(acl, Permission::Update), "ACL: {acl}");
    }
  }

  #[test]
  fn test_validate_rule() {
    assert!(validate_rule("").is_err());
    assert!(validate_rule("1, 1").is_err());
    assert!(validate_rule("1").is_ok());

    validate_rule("_USER_.id IS NOT NULL").unwrap();
    validate_rule("_USER_.id IS NOT NULL AND _ROW_.userid = _USER_.id").unwrap();
    validate_rule("_USER_.id IS NOT NULL AND _REQ_.field IS NOT NULL").unwrap();
  }
}
