use askama::Template;
use log::*;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use trailbase_schema::metadata::{
  JsonColumnMetadata, TableMetadata, ViewMetadata, find_file_column_indexes,
  find_user_id_foreign_key_columns,
};
use trailbase_schema::sqlite::Column;
use trailbase_schema::{QualifiedName, QualifiedNameEscaped};
use trailbase_sqlite::{NamedParams, Params as _, Value};

use crate::auth::user::User;
use crate::config::proto::{ConflictResolutionStrategy, RecordApiConfig};
use crate::constants::USER_TABLE;
use crate::records::params::{LazyParams, Params, prefix_colon};
use crate::records::{Permission, RecordError};

#[derive(Clone)]
pub struct RecordApi {
  state: Arc<RecordApiState>,
}

struct RecordApiSchema {
  /// Schema metadata
  qualified_name: QualifiedName,
  table_name: QualifiedNameEscaped,

  is_table: bool,
  record_pk_column: (usize, Column),
  columns: Vec<Column>,
  json_column_metadata: Vec<Option<JsonColumnMetadata>>,
  has_file_columns: bool,
  user_id_columns: Vec<usize>,

  // Helpers
  column_name_to_index: HashMap<String, usize>,
  named_params_template: NamedParams,
}

type DeferredAclCheck = dyn (FnOnce(&rusqlite::Connection) -> Result<(), RecordError>) + Send;

impl RecordApiSchema {
  fn from_table(table_metadata: &TableMetadata, config: &RecordApiConfig) -> Result<Self, String> {
    assert_name(config, table_metadata.name());

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

    let column_name_to_index = HashMap::<String, usize>::from_iter(
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
      qualified_name: table_metadata.schema.name.clone(),
      table_name: QualifiedNameEscaped::new(&table_metadata.schema.name),
      is_table: true,
      record_pk_column,
      columns,
      json_column_metadata,
      has_file_columns,
      user_id_columns,
      column_name_to_index,
      named_params_template,
    });
  }

  pub fn from_view(view_metadata: &ViewMetadata, config: &RecordApiConfig) -> Result<Self, String> {
    assert_name(config, view_metadata.name());

    let Some((pk_index, pk_column)) = view_metadata.record_pk_column() else {
      return Err(format!(
        "RecordApi requires integer/UUIDv7 primary key column: {config:?}"
      ));
    };
    let record_pk_column = (pk_index, pk_column.clone());

    let Some(columns) = view_metadata.columns() else {
      return Err("RecordApi requires schema".to_string());
    };
    let Some(ref json_metadata) = view_metadata.json_metadata else {
      return Err("RecordApi requires json metadata".to_string());
    };

    let (columns, json_column_metadata) = filter_columns(config, columns, &json_metadata.columns);

    let has_file_columns = !find_file_column_indexes(&json_column_metadata).is_empty();
    let user_id_columns = find_user_id_foreign_key_columns(&columns, USER_TABLE);

    let column_name_to_index = HashMap::<String, usize>::from_iter(
      columns
        .iter()
        .enumerate()
        .map(|(index, col)| (col.name.clone(), index)),
    );

    return Ok(Self {
      qualified_name: view_metadata.schema.name.clone(),
      table_name: QualifiedNameEscaped::new(&view_metadata.schema.name),
      is_table: false,
      record_pk_column,
      columns,
      json_column_metadata,
      has_file_columns,
      user_id_columns,
      column_name_to_index,
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

  listing_hard_limit: Option<usize>,

  // Open question: right now the read_access rule is also used for listing. It might be nice to
  // allow different permissions, however there's a risk of listing records w/o read access.
  // Arguably, this could always be modeled as two APIs with different permissions on the same
  // table.
  read_access_rule: Option<String>,
  read_access_query: Option<Arc<str>>,
  subscription_read_access_query: Option<String>,

  create_access_query: Option<Arc<str>>,
  update_access_query: Option<Arc<str>>,
  delete_access_query: Option<Arc<str>>,
  schema_access_query: Option<Arc<str>>,
}

impl RecordApiState {
  #[inline]
  fn cached_access_query(&self, p: Permission) -> Option<Arc<str>> {
    return match p {
      Permission::Create => self.create_access_query.clone(),
      Permission::Read => self.read_access_query.clone(),
      Permission::Update => self.update_access_query.clone(),
      Permission::Delete => self.delete_access_query.clone(),
      Permission::Schema => self.schema_access_query.clone(),
    };
  }
}

impl RecordApi {
  pub fn from_table(
    conn: trailbase_sqlite::Connection,
    table_metadata: &TableMetadata,
    config: RecordApiConfig,
  ) -> Result<Self, String> {
    assert_name(&config, table_metadata.name());

    return Self::from_impl(
      conn,
      RecordApiSchema::from_table(table_metadata, &config)?,
      config,
    );
  }

  pub fn from_view(
    conn: trailbase_sqlite::Connection,
    view_metadata: &ViewMetadata,
    config: RecordApiConfig,
  ) -> Result<Self, String> {
    assert_name(&config, view_metadata.name());

    return Self::from_impl(
      conn,
      RecordApiSchema::from_view(view_metadata, &config)?,
      config,
    );
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

        listing_hard_limit: config.listing_hard_limit.map(|l| l as usize),

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
  pub fn qualified_name(&self) -> &QualifiedName {
    return &self.state.schema.qualified_name;
  }

  #[inline]
  pub fn table_name(&self) -> &QualifiedNameEscaped {
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
    return self.state.schema.column_name_to_index.get(key).copied();
  }

  pub fn primary_key_to_value(&self, pk: String) -> Result<Value, RecordError> {
    // NOTE: loosly parse - will convert STRING to INT/REAL.
    return trailbase_schema::json::parse_string_to_sqlite_value(
      self.state.schema.record_pk_column.1.data_type,
      pk,
    )
    .map_err(|_| RecordError::BadRequest("Invalid id"));
  }

  #[inline]
  pub fn read_access_rule(&self) -> Option<&str> {
    return self.state.read_access_rule.as_deref();
  }

  #[inline]
  pub fn listing_hard_limit(&self) -> Option<usize> {
    return self.state.listing_hard_limit;
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
    request_params: Option<&mut LazyParams<'_>>,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    // First check table level access and if present check row-level access based on access rule.
    self.check_table_level_access(p, user)?;

    let Some(access_query) = self.state.cached_access_query(p) else {
      return Ok(());
    };

    let params = self.build_named_params(p, record_id, request_params, user)?;

    // NOTE: Avoid slushing between sqlite threads with regard to an allowed follow-on action.
    let allowed_result = match p {
      Permission::Read | Permission::Schema => {
        self
          .state
          .conn
          .call_reader(move |conn| {
            Ok(Self::check_record_level_access_impl(
              conn,
              &access_query,
              params,
            )?)
          })
          .await
      }
      _ => {
        self
          .state
          .conn
          .call(move |conn| {
            Ok(Self::check_record_level_access_impl(
              conn,
              &access_query,
              params,
            )?)
          })
          .await
      }
    };

    match allowed_result {
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

  pub fn build_record_level_access_check(
    &self,
    p: Permission,
    record_id: Option<&Value>,
    request_params: Option<&mut LazyParams<'_>>,
    user: Option<&User>,
  ) -> Result<Box<DeferredAclCheck>, RecordError> {
    // First check table level access and if present check row-level access based on access rule.
    self.check_table_level_access(p, user)?;

    let Some(access_query) = self.state.cached_access_query(p) else {
      return Ok(Box::new(|_conn| Ok(())));
    };

    let params = self.build_named_params(p, record_id, request_params, user)?;

    return Ok(Box::new(move |conn| {
      return match Self::check_record_level_access_impl(conn, &access_query, params) {
        Ok(allowed) if allowed => Ok(()),
        _ => Err(RecordError::Forbidden),
      };
    }));
  }

  #[inline]
  fn check_record_level_access_impl(
    conn: &rusqlite::Connection,
    query: &str,
    named_params: NamedParams,
  ) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare_cached(query)?;
    named_params.bind(&mut stmt)?;

    if let Some(row) = stmt.raw_query().next()? {
      return row.get(0);
    }
    return Err(rusqlite::Error::QueryReturnedNoRows);
  }

  /// Check if the given user (if any) can access a record given the request and the operation.
  #[inline]
  pub(crate) fn check_record_level_read_access_for_subscriptions(
    &self,
    conn: &rusqlite::Connection,
    params: SubscriptionAclParams<'_>,
  ) -> Result<(), RecordError> {
    // First check table level access and if present check row-level access based on access rule.
    self.check_table_level_access(Permission::Read, params.user)?;

    let Some(ref access_query) = self.state.subscription_read_access_query else {
      return Ok(());
    };

    let mut stmt = conn
      .prepare_cached(access_query)
      .map_err(|_err| RecordError::Forbidden)?;

    // NOTE: the `bind` impl does the heavy lifting.
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
  // TODO: It may be cheaper to implement trailbase_sqlite::Params for LazyParams than convert to
  // NamedParams :shrug:.
  fn build_named_params(
    &self,
    p: Permission,
    record_id: Option<&Value>,
    request_params: Option<&mut LazyParams<'_>>,
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

        let (named_params, column_names, column_indexes) = match request_params
          .ok_or_else(|| RecordError::Internal("missing insert params".into()))?
          .params()
          .map_err(|_| RecordError::BadRequest("invalid params"))?
        {
          Params::Insert {
            named_params,
            column_names,
            column_indexes,
            ..
          } => {
            assert_eq!(p, Permission::Create);
            (named_params, column_names, column_indexes)
          }
          Params::Update {
            named_params,
            column_names,
            column_indexes,
            ..
          } => {
            assert_eq!(p, Permission::Update);
            (named_params, column_names, column_indexes)
          }
        };

        assert_eq!(column_names.len(), column_indexes.len());

        // NOTE: We cannot have access queries access missing _REQ_.props. So we need to inject an
        // explicit NULL value for all missing fields on the request. Can we make this cheaper,
        // either by pre-processing the access query or improving construction?
        let mut all_named_params = self.state.schema.named_params_template.clone();

        for (index, column_index) in column_indexes.iter().enumerate() {
          // Override the default NULL value with the request value.
          all_named_params[*column_index].1 = named_params[index].1.clone();
        }

        all_named_params.push((
          Cow::Borrowed(":__fields"),
          Value::Text(serde_json::to_string(&column_names).expect("json array")),
        ));

        all_named_params
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

pub(crate) struct SubscriptionAclParams<'a> {
  pub params: &'a indexmap::IndexMap<&'a str, rusqlite::types::Value>,
  pub user: Option<&'a User>,
}

impl<'a> trailbase_sqlite::Params for SubscriptionAclParams<'a> {
  fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> rusqlite::Result<()> {
    for (name, v) in self.params {
      if let Some(idx) = stmt.parameter_index(&prefix_colon(name))? {
        stmt.raw_bind_parameter(idx, v)?;
      };
    }

    if let Some(user) = self.user
      && let Some(idx) = stmt.parameter_index(":__user_id")?
    {
      stmt.raw_bind_parameter(idx, rusqlite::types::Value::Blob(user.uuid.into()))?;
    }

    return Ok(());
  }
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
  table_name: &QualifiedNameEscaped,
  pk_column_name: &str,
  access_rule: &str,
) -> Arc<str> {
  return indoc::formatdoc!(
    r#"
      SELECT
        CAST(({access_rule}) AS INTEGER)
      FROM
        (SELECT :__user_id AS id) AS _USER_,
        (SELECT * FROM {table_name} WHERE "{pk_column_name}" = :__record_id) AS _ROW_
    "#,
  )
  .into();
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
) -> Result<Arc<str>, String> {
  let column_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();

  return Ok(
    CreateRecordAccessQueryTemplate {
      create_access_rule,
      column_names,
    }
    .render()
    .map_err(|err| err.to_string())?
    .into(),
  );
}

#[derive(Template)]
#[template(
  escape = "none",
  whitespace = "minimize",
  path = "update_record_access_query.sql"
)]
struct UpdateRecordAccessQueryTemplate<'a> {
  update_access_rule: &'a str,
  table_name: &'a QualifiedNameEscaped,
  pk_column_name: &'a str,
  column_names: Vec<&'a str>,
}

/// Build access query for record updates.
///
/// Assumes access_rule is an expression: https://www.sqlite.org/syntax/expr.html
fn build_update_access_query(
  table_name: &QualifiedNameEscaped,
  columns: &[Column],
  pk_column_name: &str,
  update_access_rule: &str,
) -> Result<Arc<str>, String> {
  let column_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();

  return Ok(
    UpdateRecordAccessQueryTemplate {
      update_access_rule,
      table_name,
      pk_column_name,
      column_names,
    }
    .render()
    .map_err(|err| err.to_string())?
    .into(),
  );
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
  config: &RecordApiConfig,
  columns: &[Column],
  json_column_metadata: &[Option<JsonColumnMetadata>],
) -> (Vec<Column>, Vec<Option<JsonColumnMetadata>>) {
  assert_eq!(columns.len(), json_column_metadata.len());
  if config.excluded_columns.is_empty() {
    return (columns.to_vec(), json_column_metadata.to_vec());
  }

  let excluded_indexes = config
    .excluded_columns
    .iter()
    .filter_map(|name| columns.iter().position(|col| col.name == *name));
  assert_eq!(
    excluded_indexes.clone().count(),
    config.excluded_columns.len()
  );

  let mut columns_vec = columns.to_vec();
  let mut json_column_metadata_vec = json_column_metadata.to_vec();
  for idx in excluded_indexes.rev() {
    columns_vec.remove(idx);
    json_column_metadata_vec.remove(idx);
  }

  return (columns_vec, json_column_metadata_vec);
}

#[inline]
fn assert_name(config: &RecordApiConfig, name: &QualifiedName) {
  // TODO: Should this be disabled in prod?
  if let Some(ref db) = name.database_schema
    && db != "main"
  {
    assert_eq!(
      config.table_name.as_deref().unwrap_or_default(),
      format!("{db}.{}", name.name)
    );
  } else {
    assert_eq!(config.table_name.as_deref().unwrap_or_default(), &name.name);
  }
}

#[cfg(test)]
mod tests {
  use trailbase_schema::parse::parse_into_statement;
  use trailbase_schema::sqlite::QualifiedName;

  use super::*;
  use crate::{config::proto::PermissionFlag, records::Permission};

  fn sanitize_template(template: &str) {
    assert!(parse_into_statement(template).is_ok(), "{template}");
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
        table_name: &QualifiedName::parse("table").unwrap().into(),
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
        table_name: &QualifiedName::parse("table").unwrap().into(),
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
}
