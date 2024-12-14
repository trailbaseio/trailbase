use itertools::Itertools;
use log::*;
use std::sync::Arc;
use trailbase_sqlite::Params as _;

use crate::auth::user::User;
use crate::config::proto::{ConflictResolutionStrategy, RecordApiConfig};
use crate::records::json_to_sql::{LazyParams, Params};
use crate::records::{Permission, RecordError};
use crate::schema::{Column, ColumnDataType};
use crate::table_metadata::{TableMetadata, TableOrViewMetadata, ViewMetadata};
use crate::util::{assert_uuidv7, b64_to_id};

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

enum RecordApiMetadata {
  Table(TableMetadata),
  View(ViewMetadata),
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

  create_access_rule: Option<String>,
  read_access_rule: Option<String>,
  update_access_rule: Option<String>,
  delete_access_rule: Option<String>,
  schema_access_rule: Option<String>,
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

        // Access control lists.
        acl: [
          convert_acl(&config.acl_world),
          convert_acl(&config.acl_authenticated),
        ],
        // Access rules.
        create_access_rule: config.create_access_rule,
        read_access_rule: config.read_access_rule,
        update_access_rule: config.update_access_rule,
        delete_access_rule: config.delete_access_rule,
        schema_access_rule: config.schema_access_rule,
      }),
    });
  }

  #[inline]
  pub fn api_name(&self) -> &str {
    &self.state.api_name
  }

  #[inline]
  pub fn table_name(&self) -> &str {
    match &self.state.metadata {
      RecordApiMetadata::Table(ref table) => &table.schema.name,
      RecordApiMetadata::View(ref view) => &view.schema.name,
    }
  }

  #[inline]
  pub fn metadata(&self) -> &(dyn TableOrViewMetadata + Send + Sync) {
    match &self.state.metadata {
      RecordApiMetadata::Table(ref table) => table,
      RecordApiMetadata::View(ref view) => view,
    }
  }

  pub fn table_metadata(&self) -> Option<&TableMetadata> {
    match &self.state.metadata {
      RecordApiMetadata::Table(ref table) => Some(table),
      RecordApiMetadata::View(ref _view) => None,
    }
  }

  pub fn id_to_sql(&self, id: &str) -> Result<trailbase_sqlite::Value, RecordError> {
    return match self.state.record_pk_column.data_type {
      ColumnDataType::Blob => {
        let record_id = b64_to_id(id).map_err(|_err| RecordError::BadRequest("Invalid id"))?;
        assert_uuidv7(&record_id);
        Ok(trailbase_sqlite::Value::Blob(record_id.into()))
      }
      ColumnDataType::Integer => Ok(trailbase_sqlite::Value::Integer(
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
  pub fn access_rule(&self, p: Permission) -> &Option<String> {
    return match p {
      Permission::Create => &self.state.create_access_rule,
      Permission::Read => &self.state.read_access_rule,
      Permission::Update => &self.state.update_access_rule,
      Permission::Delete => &self.state.delete_access_rule,
      Permission::Schema => &self.state.schema_access_rule,
    };
  }

  #[inline]
  pub fn insert_autofill_missing_user_id_columns(&self) -> bool {
    return self.state.insert_autofill_missing_user_id_columns;
  }

  #[inline]
  pub fn insert_conflict_resolution_strategy(&self) -> Option<ConflictResolutionStrategy> {
    return self.state.insert_conflict_resolution_strategy;
  }

  /// Check if the given user (if any) can access a record given the request and the operation.
  pub async fn check_record_level_access(
    &self,
    p: Permission,
    record_id: Option<&trailbase_sqlite::Value>,
    request_params: Option<&mut LazyParams<'_>>,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    // First check table level access and if present check row-level access based on access rule.
    self.check_table_level_access(p, user)?;

    'acl: {
      let Some(ref access_rule) = self.access_rule(p) else {
        return Ok(());
      };

      let (access_query, params) = self.build_access_query_and_params(
        p,
        access_rule,
        self.table_name(),
        record_id,
        request_params,
        user,
      )?;

      let allowed = match self
        .state
        .conn
        .call(move |conn| Self::query_access(conn, &access_query, params))
        .await
      {
        Ok(allowed) => allowed,
        Err(err) => {
          if cfg!(test) {
            panic!("RLA query failed: {err}");
          }
          warn!("RLA query failed: {err}");
          break 'acl;
        }
      };

      if allowed {
        return Ok(());
      }
    }

    return Err(RecordError::Forbidden);
  }

  /// Check if the given user (if any) can access a record given the request and the operation.
  #[allow(unused)]
  pub fn check_record_level_access_sync(
    &self,
    conn: &mut rusqlite::Connection,
    p: Permission,
    record_id: Option<&trailbase_sqlite::Value>,
    request_params: Option<&mut LazyParams<'_>>,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    // First check table level access and if present check row-level access based on access rule.
    self.check_table_level_access(p, user)?;

    'acl: {
      let Some(ref access_rule) = self.access_rule(p) else {
        return Ok(());
      };

      let (access_query, params) = self.build_access_query_and_params(
        p,
        access_rule,
        self.table_name(),
        record_id,
        request_params,
        user,
      )?;

      let allowed = match Self::query_access(conn, &access_query, params) {
        Ok(allowed) => allowed,
        Err(err) => {
          if cfg!(test) {
            panic!("RLA query failed: {err}");
          }
          warn!("RLA query failed: {err}");
          break 'acl;
        }
      };

      if allowed {
        return Ok(());
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
  fn query_access(
    conn: &mut rusqlite::Connection,
    access_query: &str,
    params: Vec<(String, trailbase_sqlite::Value)>,
  ) -> Result<bool, trailbase_sqlite::Error> {
    let mut stmt = conn.prepare(access_query)?;
    params.bind(&mut stmt)?;

    let mut rows = stmt.raw_query();
    if let Some(row) = rows.next()? {
      return Ok(row.get(0)?);
    }

    return Err(rusqlite::Error::QueryReturnedNoRows.into());
  }

  #[inline]
  fn has_access(&self, e: Entity, p: Permission) -> bool {
    return (self.state.acl[e as usize] & (p as u8)) > 0;
  }

  // TODO: We should probably break this up into separate functions for CRUD, to only do and inject
  // what's actually needed. Maybe even break up the entire check_access_and_rls_then. It's pretty
  // winding right now.
  fn build_access_query_and_params(
    &self,
    p: Permission,
    access_rule: &str,
    table_name: &str,
    record_id: Option<&trailbase_sqlite::Value>,
    request_params: Option<&mut LazyParams<'_>>,
    user: Option<&User>,
  ) -> Result<(String, Vec<(String, trailbase_sqlite::Value)>), RecordError> {
    let pk_column_name = &self.state.record_pk_column.name;
    // We need to inject context like: record id, user, request, and row into the access
    // check. Below we're building the query and binding the context as params accordingly.
    let (user_sub_select, mut params) = build_user_sub_select(user);

    params.push((
      ":__record_id".to_string(),
      record_id.map_or(trailbase_sqlite::Value::Null, |id| id.clone()),
    ));

    // Assumes access_rule is an expression: https://www.sqlite.org/syntax/expr.html
    //
    // Create has no "row"
    // Read and delete have no "request"
    // And only update has "row" and "request".
    let query = match p {
      Permission::Create => {
        let Some(table_metadata) = self.table_metadata() else {
          return Err(RecordError::ApiRequiresTable);
        };

        let (request_sub_select, mut request_params) = build_request_sub_select(
          table_metadata,
          request_params
            .unwrap()
            .params()
            .map_err(|err| RecordError::Internal(err.into()))?,
        );
        params.append(&mut request_params);

        indoc::formatdoc!(
          r#"
          SELECT
            ({access_rule})
          FROM
            ({user_sub_select}) AS _USER_,
            ({request_sub_select}) AS _REQ_
        "#,
        )
      }
      Permission::Update => {
        let Some(table_metadata) = self.table_metadata() else {
          return Err(RecordError::ApiRequiresTable);
        };

        let (request_sub_select, mut request_params) = build_request_sub_select(
          table_metadata,
          request_params
            .unwrap()
            .params()
            .map_err(|err| RecordError::Internal(err.into()))?,
        );
        params.append(&mut request_params);

        indoc::formatdoc!(
          r#"
          SELECT
            ({access_rule})
          FROM
            ({user_sub_select}) AS _USER_,
            ({request_sub_select}) AS _REQ_,
            (SELECT * FROM "{table_name}" WHERE "{pk_column_name}" = :__record_id) AS _ROW_
        "#,
        )
      }
      Permission::Read | Permission::Delete | Permission::Schema => indoc::formatdoc!(
        r#"
          SELECT
            ({access_rule})
          FROM
            ({user_sub_select}) AS _USER_,
            (SELECT * FROM "{table_name}" WHERE "{pk_column_name}" = :__record_id) AS _ROW_
        "#
      ),
    };

    return Ok((query, params));
  }
}

pub(crate) fn build_user_sub_select(
  user: Option<&User>,
) -> (&'static str, Vec<(String, trailbase_sqlite::Value)>) {
  const QUERY: &str = "SELECT :__user_id AS id";

  if let Some(user) = user {
    return (
      QUERY,
      vec![(
        ":__user_id".to_string(),
        trailbase_sqlite::Value::Blob(user.uuid.into()),
      )],
    );
  } else {
    return (
      QUERY,
      vec![(":__user_id".to_string(), trailbase_sqlite::Value::Null)],
    );
  }
}

/// Builds the sub-query for _REQ_.
fn build_request_sub_select(
  table_metadata: &TableMetadata,
  request_params: &Params,
) -> (String, Vec<(String, trailbase_sqlite::Value)>) {
  // NOTE: This has gotten pretty wild. We cannot have access queries access missing _REQ_.props.
  // So we need to inject an explicit NULL value for all missing fields on the request.
  // Can we make this cheaper, either by pre-processing the access query or improving construction?
  // For example, could we build a transaction-scoped temp view with positional placeholders to
  // save some string ops?
  let schema = &table_metadata.schema;

  let mut named_params: Vec<(String, trailbase_sqlite::Value)> = schema
    .columns
    .iter()
    .map(|c| (format!(":{}", c.name), trailbase_sqlite::Value::Null))
    .collect();

  for (param_index, col_name) in request_params.column_names().iter().enumerate() {
    let Some(col_index) = table_metadata.column_index_by_name(col_name) else {
      // We simply skip unknown columns, this could simply be malformed input or version skew. This
      // is similar in spirit to protobuf's unknown fields behavior.
      continue;
    };

    named_params[col_index].1 = request_params.named_params()[param_index].1.clone();
  }

  return (
    format!(
      "SELECT {placeholders}",
      placeholders = schema
        .columns
        .iter()
        .map(|col| format!(":{col_name} AS '{col_name}'", col_name = col.name))
        .join(", ")
    ),
    named_params,
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

#[cfg(test)]
mod tests {
  use super::convert_acl;
  use crate::{config::proto::PermissionFlag, records::Permission};

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
