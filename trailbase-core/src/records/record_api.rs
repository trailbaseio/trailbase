use askama::Template;
use rusqlite::types::ToSqlOutput;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::*;
use trailbase_sqlite::{NamedParamRef, NamedParams, Params as _, Value};

use crate::auth::user::User;
use crate::config::proto::{ConflictResolutionStrategy, RecordApiConfig};
use crate::records::params::{prefix_colon, LazyParams, Params};
use crate::records::{Permission, RecordError};
use crate::schema::{Column, ColumnDataType};
use crate::table_metadata::{TableMetadata, TableOrViewMetadata, ViewMetadata};
use crate::util::{assert_uuidv7, b64_to_id};

enum RecordApiMetadata {
  Table(TableMetadata),
  View(ViewMetadata),
}

impl RecordApiMetadata {
  #[inline]
  fn table_name(&self) -> &str {
    match &self {
      RecordApiMetadata::Table(table) => &table.schema.name,
      RecordApiMetadata::View(view) => &view.schema.name,
    }
  }

  #[inline]
  fn metadata(&self) -> &(dyn TableOrViewMetadata + Send + Sync) {
    match &self {
      RecordApiMetadata::Table(table) => table,
      RecordApiMetadata::View(view) => view,
    }
  }
}

/// FILTER CONTROL.
///
/// Open question: right now we use the read_access rule also for listing. It could be nice to
/// allow different access rules. On the other hand, this could also lead to setups where you can
/// list records you cannot read (the other way round might be more sensible).
/// On the other hand, different permissions could also be modeled as multiple apis on the same
/// table.
///
/// Independently, listing a user's own items might be a common task. Should we support a magic
/// filter "mine" or is "owner_col=<my_user_id>" good enough?
#[derive(Clone)]
pub struct RecordApi {
  state: Arc<RecordApiState>,
}

struct RecordApiState {
  conn: trailbase_sqlite::Connection,
  metadata: RecordApiMetadata,
  record_pk_column: Column,

  // Below properties are filled from `proto::RecordApiConfig`.
  api_name: String,
  acl: [u8; 2],
  insert_conflict_resolution_strategy: Option<ConflictResolutionStrategy>,
  insert_autofill_missing_user_id_columns: bool,
  enable_subscriptions: bool,

  expand: Option<HashMap<String, serde_json::Value>>,

  create_access_rule: Option<String>,
  create_access_query: Option<String>,

  read_access_rule: Option<String>,
  read_access_query: Option<String>,
  subscription_read_access_query: Option<String>,

  update_access_rule: Option<String>,
  update_access_query: Option<String>,

  delete_access_rule: Option<String>,
  delete_access_query: Option<String>,

  schema_access_rule: Option<String>,
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
    table_metadata: TableMetadata,
    config: RecordApiConfig,
  ) -> Result<Self, String> {
    let Some(ref table_name) = config.table_name else {
      return Err(format!(
        "RecordApi misses table_name configuration: {config:?}"
      ));
    };
    if table_name != table_metadata.name() {
      return Err(format!(
        "Expected table name '{table_name}', got: {}",
        table_metadata.name()
      ));
    }

    let Some((_index, record_pk_column)) = table_metadata.record_pk_column() else {
      return Err(format!(
        "RecordApi requires integer/UUIDv7 primary key column: {config:?}"
      ));
    };

    return Self::from_impl(
      conn,
      record_pk_column.clone(),
      RecordApiMetadata::Table(table_metadata),
      config,
    );
  }

  pub fn from_view(
    conn: trailbase_sqlite::Connection,
    view_metadata: ViewMetadata,
    config: RecordApiConfig,
  ) -> Result<Self, String> {
    let Some(ref table_name) = config.table_name else {
      return Err(format!(
        "RecordApi misses table_name configuration: {config:?}"
      ));
    };
    if table_name != view_metadata.name() {
      return Err(format!(
        "Expected table name '{table_name}', got: {}",
        view_metadata.name()
      ));
    }

    if view_metadata.schema.temporary {
      return Err(format!(
        "RecordAPIs cannot point to temporary view: {table_name}",
      ));
    }

    let Some((_index, record_pk_column)) = view_metadata.record_pk_column() else {
      return Err(format!(
        "RecordApi requires integer/UUIDv7 primary key column: {config:?}"
      ));
    };

    return Self::from_impl(
      conn,
      record_pk_column.clone(),
      RecordApiMetadata::View(view_metadata),
      config,
    );
  }

  fn from_impl(
    conn: trailbase_sqlite::Connection,
    record_pk_column: Column,
    metadata: RecordApiMetadata,
    config: RecordApiConfig,
  ) -> Result<Self, String> {
    let Some(api_name) = config.name.clone() else {
      return Err(format!("RecordApi misses name: {config:?}"));
    };

    let (read_access_query, subscription_read_access_query) = match &config.read_access_rule {
      Some(rule) => {
        let read_access_query =
          build_read_delete_schema_query(metadata.table_name(), &record_pk_column.name, rule);

        let subscription_read_access_query = match metadata {
          RecordApiMetadata::Table(ref m) => Some(
            SubscriptionRecordReadTemplate {
              read_access_rule: rule,
              column_names: m.schema.columns.iter().map(|c| c.name.as_str()).collect(),
            }
            .render()
            .map_err(|err| err.to_string())?,
          ),
          _ => None,
        };

        (Some(read_access_query), subscription_read_access_query)
      }
      None => (None, None),
    };

    let delete_access_query = config.delete_access_rule.as_ref().map(|rule| {
      build_read_delete_schema_query(metadata.table_name(), &record_pk_column.name, rule)
    });

    let schema_access_query = config.schema_access_rule.as_ref().map(|rule| {
      build_read_delete_schema_query(metadata.table_name(), &record_pk_column.name, rule)
    });

    let create_access_query = match &config.create_access_rule {
      Some(rule) => match metadata {
        RecordApiMetadata::Table(ref m) => Some(build_create_access_query(m, rule)?),
        _ => None,
      },
      None => None,
    };

    let update_access_query = match &config.update_access_rule {
      Some(rule) => match metadata {
        RecordApiMetadata::Table(ref m) => {
          Some(build_update_access_query(m, &record_pk_column.name, rule)?)
        }
        _ => None,
      },
      None => None,
    };

    return Ok(RecordApi {
      state: Arc::new(RecordApiState {
        conn,
        metadata,
        record_pk_column,
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
        create_access_rule: config.create_access_rule,
        create_access_query,

        read_access_rule: config.read_access_rule,
        read_access_query,
        subscription_read_access_query,

        update_access_rule: config.update_access_rule,
        update_access_query,

        delete_access_rule: config.delete_access_rule,
        delete_access_query,

        schema_access_rule: config.schema_access_rule,
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
    return self.state.metadata.table_name();
  }

  #[inline]
  pub fn metadata(&self) -> &(dyn TableOrViewMetadata + Send + Sync) {
    return self.state.metadata.metadata();
  }

  #[inline]
  pub(crate) fn expand(&self) -> Option<&HashMap<String, serde_json::Value>> {
    return self.state.expand.as_ref();
  }

  pub fn table_metadata(&self) -> Option<&TableMetadata> {
    match &self.state.metadata {
      RecordApiMetadata::Table(table) => Some(table),
      RecordApiMetadata::View(_view) => None,
    }
  }

  pub fn id_to_sql(&self, id: &str) -> Result<Value, RecordError> {
    return match self.state.record_pk_column.data_type {
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
  pub fn record_pk_column(&self) -> &Column {
    return &self.state.record_pk_column;
  }

  #[inline]
  pub fn access_rule(&self, p: Permission) -> Option<&str> {
    return match p {
      Permission::Create => self.state.create_access_rule.as_deref(),
      Permission::Read => self.state.read_access_rule.as_deref(),
      Permission::Update => self.state.update_access_rule.as_deref(),
      Permission::Delete => self.state.delete_access_rule.as_deref(),
      Permission::Schema => self.state.schema_access_rule.as_deref(),
    };
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
    request_params: Option<&mut LazyParams<'_>>,
    user: Option<&User>,
  ) -> Result<NamedParams, RecordError> {
    // We need to inject context like: record id, user, request, and row into the access
    // check. Below we're building the query and binding the context as params accordingly.
    let mut params = match p {
      Permission::Create | Permission::Update => {
        let Some(table_metadata) = self.table_metadata() else {
          return Err(RecordError::ApiRequiresTable);
        };

        let request_params = request_params
          .ok_or_else(|| RecordError::Internal("missing req params".into()))?
          .params()
          .map_err(|err| RecordError::Internal(err.into()))?;

        build_request_params(table_metadata, request_params)
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
        ({access_rule})
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
  table_metadata: &TableMetadata,
  create_access_rule: &str,
) -> Result<String, String> {
  let column_names: Vec<&str> = table_metadata
    .schema
    .columns
    .iter()
    .map(|c| c.name.as_str())
    .collect();

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
  table_metadata: &TableMetadata,
  pk_column_name: &str,
  update_access_rule: &str,
) -> Result<String, String> {
  let table_name = table_metadata.name();
  let column_names: Vec<&str> = table_metadata
    .schema
    .columns
    .iter()
    .map(|c| c.name.as_str())
    .collect();

  return UpdateRecordAccessQueryTemplate {
    update_access_rule,
    table_name,
    pk_column_name,
    column_names,
  }
  .render()
  .map_err(|err| err.to_string());
}

/// Build SQL named parameters from request fields.
#[inline]
fn build_request_params(table_metadata: &TableMetadata, request_params: &Params) -> NamedParams {
  // NOTE: We cannot have access queries access missing _REQ_.props. So we need to inject an
  // explicit NULL value for all missing fields on the request. Can we make this cheaper, either by
  // pre-processing the access query or improving construction?
  let mut named_params = table_metadata.named_params_template.clone();

  // 'outer: for (placeholder, value) in &request_params.named_params {
  //   for (p, ref mut v) in named_params.iter_mut() {
  //     if *placeholder == *p {
  //       *v = value.clone();
  //       continue 'outer;
  //     }
  //   }
  // }

  for (param_index, col_name) in request_params.column_names.iter().enumerate() {
    let Some(col_index) = table_metadata.column_index_by_name(col_name) else {
      // We simply skip unknown columns, this could simply be malformed input or version skew. This
      // is similar in spirit to protobuf's unknown fields behavior.
      continue;
    };

    named_params[col_index].1 = request_params.named_params[param_index].1.clone();
  }

  return named_params;
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

#[cfg(test)]
mod tests {
  use super::*;

  use crate::table_metadata::sqlite3_parse_into_statement;
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
}
