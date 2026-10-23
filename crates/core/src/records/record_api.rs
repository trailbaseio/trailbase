use askama::Template;
use compact_str::CompactString;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use trailbase_schema::db::metadata::{
  ColumnMetadata, ConnectionMetadata, TableMetadata, ViewMetadata, find_file_column_indexes,
  find_user_id_foreign_key_columns,
};
use trailbase_schema::db::{QualifiedName, QualifiedNameEscaped};
use trailbase_sqlite::to_sql::ToSqlProxy;
use trailbase_sqlite::{Connection, ConnectionType, SyncConnectionTrait, Value, ValueRef};

use crate::auth::user::User;
use crate::config::proto;
use crate::constants::USER_TABLE;
use crate::records::params::Params;
use crate::records::subscribe::event::EventPayload;
use crate::records::util::named_placeholder;
use crate::records::{Permission, RecordError};

macro_rules! debug_is_some {
  ($e:expr) => {{
    let opt = $e;
    debug_assert!(opt.is_some());
    opt
  }};
}

#[derive(Debug)]
pub(crate) struct RecordApiSchema {
  /// Schema metadata
  qualified_name: QualifiedName,
  table_name: QualifiedNameEscaped,
  attached_databases: Vec<String>,

  is_table: bool,
  record_pk_column: ColumnMetadata,
  column_metadata: Vec<ColumnMetadata>,

  has_file_columns: bool,
  user_id_columns: Vec<usize>,

  // Helpers:

  // A cheaply clone-able list of all column names. Used for insert/update param construction.
  column_names: Arc<Vec<CompactString>>,

  // Mapping form column name to *API column index*.
  //
  // NOTE: The index may be different from the column's index in the underlying TABLE or VIEW
  // due to excluded columns.
  column_name_to_index: HashMap<String, usize>,
}

impl RecordApiSchema {
  fn from_table(
    table_metadata: &TableMetadata,
    config: &proto::RecordApiConfig,
  ) -> Result<Self, String> {
    assert_name(config, table_metadata.name());

    let Some(record_pk_column) = table_metadata.record_pk_column() else {
      return Err("RecordApi requires integer/UUIDv7 primary key column".into());
    };

    let column_metadata = filter_excluded_columns(config, &table_metadata.column_metadata);
    let has_file_columns = !find_file_column_indexes(&column_metadata).is_empty();
    let user_id_columns = find_user_id_foreign_key_columns(&column_metadata, USER_TABLE);

    let column_name_to_index = HashMap::<String, usize>::from_iter(
      column_metadata
        .iter()
        .enumerate()
        .map(|(index, meta)| (meta.column.name.clone(), index)),
    );

    let column_names = Arc::new(
      column_metadata
        .iter()
        .map(|meta| named_placeholder(&meta.column.name))
        .collect(),
    );

    return Ok(Self {
      qualified_name: table_metadata.schema.name.clone(),
      table_name: QualifiedNameEscaped::new(&table_metadata.schema.name),
      attached_databases: config.attached_databases.clone(),
      is_table: true,
      record_pk_column: record_pk_column.clone(),
      column_metadata,
      has_file_columns,
      user_id_columns,
      column_name_to_index,
      column_names,
    });
  }

  fn from_view(
    view_metadata: &ViewMetadata,
    config: &proto::RecordApiConfig,
  ) -> Result<Self, String> {
    assert_name(config, view_metadata.name());

    let Some(record_pk_column) = view_metadata.record_pk_column() else {
      return Err(format!(
        "RecordApi requires integer/UUIDv7 primary key column: {config:?}"
      ));
    };

    let Some(ref column_metadata) = view_metadata.column_metadata else {
      return Err("RecordApi requires schema".to_string());
    };

    let column_metadata = filter_excluded_columns(config, column_metadata);
    let has_file_columns = !find_file_column_indexes(&column_metadata).is_empty();
    let user_id_columns = find_user_id_foreign_key_columns(&column_metadata, USER_TABLE);

    let column_name_to_index = HashMap::<String, usize>::from_iter(
      column_metadata
        .iter()
        .enumerate()
        .map(|(index, meta)| (meta.column.name.clone(), index)),
    );

    let column_names = Arc::new(
      column_metadata
        .iter()
        .map(|meta| named_placeholder(&meta.column.name))
        .collect(),
    );

    return Ok(Self {
      qualified_name: view_metadata.schema.name.clone(),
      table_name: QualifiedNameEscaped::new(&view_metadata.schema.name),
      attached_databases: config.attached_databases.clone(),
      is_table: false,
      record_pk_column: record_pk_column.clone(),
      column_metadata: column_metadata.clone(),
      has_file_columns,
      user_id_columns,
      column_name_to_index,
      column_names,
    });
  }
}

#[derive(Clone)]
pub struct RecordApi {
  state: Arc<RecordApiState>,
}

struct RequestParams<'a> {
  all_column_names: Arc<Vec<CompactString>>,
  present_fields: String,
  request_params: Params<'a>,
}

impl<'a> RequestParams<'a> {
  fn new(all_column_names: Arc<Vec<CompactString>>, request_params: Params<'a>) -> Self {
    let present_fields = match request_params {
      Params::Insert(p) => strings_to_json_array(p.params.iter().map(|p| p.column_name.as_str())),
      Params::Update(p) => strings_to_json_array(p.params.iter().map(|p| p.column_name.as_str())),
    };

    return Self {
      all_column_names,
      present_fields,
      request_params,
    };
  }
}

struct RecordAclParams<'a> {
  user: Option<[u8; 16]>,
  tenant: Option<[u8; 16]>,
  record_id: Option<Value>,
  params: Option<RequestParams<'a>>,
}

impl<'a> RecordAclParams<'a> {
  fn new(
    p: Permission,
    user: Option<&User>,
    record_id: Option<Value>,
    all_column_names: &Arc<Vec<CompactString>>,
    request_params: Option<Params<'a>>,
  ) -> Result<Self, RecordError> {
    return Ok(Self {
      user: user.map(|u| u.id.into_bytes()),
      tenant: user.and_then(|u| u.tenant.map(|t| t.into_bytes())),
      record_id,
      params: match p {
        Permission::Create | Permission::Update => Some(RequestParams::new(
          all_column_names.clone(),
          request_params
            .ok_or_else(|| RecordError::Internal("missing insert/update params".into()))?,
        )),
        Permission::Read | Permission::Delete | Permission::Schema => None,
      },
    });
  }
}

impl<'a> trailbase_sqlite::Params for RecordAclParams<'a> {
  fn bind<S: trailbase_sqlite::Statement>(
    self,
    stmt: &mut S,
  ) -> Result<(), trailbase_sqlite::Error> {
    let mut is_insert = false;
    if let Some(params) = self.params {
      if let Some(idx) = debug_is_some!(stmt.parameter_index(":__fields")?) {
        stmt.bind_parameter(idx, ToSqlProxy::Owned(Value::Text(params.present_fields)))?;
      }

      let request_params = match params.request_params {
        Params::Insert(p) => {
          is_insert = true;
          &p.params
        }
        Params::Update(p) => &p.params,
      };

      // TODO: We could first sort the `request_params` by column index and then move i, j indexes
      // in lockstep to avoid repeat scans and move more work off the executor.
      for (column_index, column_name) in params.all_column_names.iter().enumerate() {
        if let Some(idx) = debug_is_some!(stmt.parameter_index(column_name)?) {
          stmt.bind_parameter(
            idx,
            request_params
              .iter()
              .find_map(|p| {
                // NOTE: Consider matching here by column_name and removing the index.
                if p.column_index == column_index {
                  debug_assert_eq!(p.placeholder, *column_name);
                  Some(ToSqlProxy::Borrowed((&p.value).into()))
                } else {
                  None
                }
              })
              .unwrap_or(ToSqlProxy::Borrowed(ValueRef::Null)),
          )?;
        }
      }
    }

    if let Some(idx) = debug_is_some!(stmt.parameter_index(":__user_id")?) {
      if let Some(user) = self.user {
        stmt.bind_parameter(idx, ToSqlProxy::Borrowed(ValueRef::Blob(&user)))?;
      } else {
        stmt.bind_parameter(idx, ToSqlProxy::Borrowed(ValueRef::Null))?;
      }
    }

    if let Some(idx) = debug_is_some!(stmt.parameter_index(":__tenant_id")?) {
      if let Some(tenant) = self.tenant {
        stmt.bind_parameter(idx, ToSqlProxy::Borrowed(ValueRef::Blob(&tenant)))?;
      } else {
        stmt.bind_parameter(idx, ToSqlProxy::Borrowed(ValueRef::Null))?;
      }
    }

    if !is_insert && let Some(idx) = debug_is_some!(stmt.parameter_index(":__record_id")?) {
      if let Some(id) = self.record_id {
        stmt.bind_parameter(idx, ToSqlProxy::Owned(id))?;
      } else {
        stmt.bind_parameter(idx, ToSqlProxy::Borrowed(ValueRef::Null))?;
      }
    }

    return Ok(());
  }
}

struct RecordApiState {
  /// Cached connection for access checks and subscription state construction.
  conn: trailbase_sqlite::Connection,
  /// Cached connection metadata.
  metadata: Arc<ConnectionMetadata>,

  /// Schema metadata
  schema: RecordApiSchema,

  // Below properties are filled from `proto::RecordApiConfig`.
  api_name: String,
  acl: [u8; 2],
  insert_conflict_resolution_strategy: Option<proto::ConflictResolutionStrategy>,
  insert_autofill_missing_user_id_columns: bool,
  enable_subscriptions: bool,

  // Foreign key expansion configuration. Affects schema.
  expand: Vec<CompactString>,

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
  pub(crate) fn from_table(
    conn: Connection,
    metadata: Arc<ConnectionMetadata>,
    table_metadata: &TableMetadata,
    config: proto::RecordApiConfig,
  ) -> Result<Self, String> {
    assert_name(&config, table_metadata.name());

    return Self::from_impl(
      conn,
      metadata,
      RecordApiSchema::from_table(table_metadata, &config)?,
      config,
    );
  }

  pub(crate) fn from_view(
    conn: Connection,
    metadata: Arc<ConnectionMetadata>,
    view_metadata: &ViewMetadata,
    config: proto::RecordApiConfig,
  ) -> Result<Self, String> {
    assert_name(&config, view_metadata.name());

    return Self::from_impl(
      conn,
      metadata,
      RecordApiSchema::from_view(view_metadata, &config)?,
      config,
    );
  }

  fn from_impl(
    conn: Connection,
    metadata: Arc<ConnectionMetadata>,
    schema: RecordApiSchema,
    config: proto::RecordApiConfig,
  ) -> Result<Self, String> {
    let Some(api_name) = config.name.clone() else {
      return Err(format!("RecordApi misses name: {config:?}"));
    };

    let (read_access_query, subscription_read_access_query) = match &config.read_access_rule {
      Some(rule) => {
        let read_access_query = build_read_delete_schema_query(
          conn.connection_type(),
          &schema.table_name,
          &schema.record_pk_column.column.name,
          rule,
        );

        let subscription_read_access_query = if schema.is_table {
          Some(
            SubscriptionRecordReadTemplate {
              read_access_rule: rule,
              column_names: schema
                .column_metadata
                .iter()
                .map(|m| m.column.name.as_str())
                .collect(),
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
      build_read_delete_schema_query(
        conn.connection_type(),
        &schema.table_name,
        &schema.record_pk_column.column.name,
        rule,
      )
    });

    let schema_access_query = config.schema_access_rule.as_ref().map(|rule| {
      build_read_delete_schema_query(
        conn.connection_type(),
        &schema.table_name,
        &schema.record_pk_column.column.name,
        rule,
      )
    });

    let create_access_query = match &config.create_access_rule {
      Some(rule) => {
        if schema.is_table {
          Some(build_create_access_query(
            conn.connection_type(),
            &schema.column_metadata,
            rule,
          )?)
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
            conn.connection_type(),
            &schema.table_name,
            &schema.column_metadata,
            &schema.record_pk_column.column.name,
            rule,
          )?)
        } else {
          None
        }
      }
      None => None,
    };

    return Ok(RecordApiState {
      conn,
      metadata,

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
        vec![]
      } else {
        config
          .expand
          .iter()
          .map(|col_name| col_name.into())
          .collect()
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
    });
  }

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
  pub(crate) fn build(
    conn: trailbase_sqlite::Connection,
    metadata: Arc<trailbase_schema::db::metadata::ConnectionMetadata>,
    config: proto::RecordApiConfig,
  ) -> Result<Self, String> {
    let table_name = QualifiedName::parse(config.table_name()).map_err(|err| err.to_string())?;

    if let Some(table_metadata) = metadata.get_table(&table_name) {
      return Ok(Self {
        state: Arc::new(RecordApiState::from_table(
          conn,
          metadata.clone(),
          table_metadata,
          config,
        )?),
      });
    } else if let Some(view_metadata) = metadata.get_view(&table_name) {
      return Ok(Self {
        state: Arc::new(RecordApiState::from_view(
          conn,
          metadata.clone(),
          view_metadata,
          config,
        )?),
      });
    }

    return Err(format!(
      "RecordApi references missing table/view: {config:?}"
    ));
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

  pub fn attached_databases(&self) -> &[String] {
    return &self.state.schema.attached_databases;
  }

  pub fn conn(&self) -> &trailbase_sqlite::Connection {
    return &self.state.conn;
  }

  // NOTE: We use this for expansions when we follow FKs (read, list, json schema) as well as
  // constructing per-connection subscription state (though this could probably be untangled).
  pub(crate) fn connection_metadata(&self) -> &Arc<ConnectionMetadata> {
    return &self.state.metadata;
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
  pub(crate) fn expand(&self) -> &[CompactString] {
    return self.state.expand.as_ref();
  }

  #[inline]
  pub fn record_pk_column(&self) -> &ColumnMetadata {
    return &self.state.schema.record_pk_column;
  }

  #[inline]
  pub fn columns(&self) -> &[ColumnMetadata] {
    return &self.state.schema.column_metadata;
  }

  #[inline]
  pub fn is_table(&self) -> bool {
    return self.state.schema.is_table;
  }

  #[inline]
  pub fn column_index_by_name(&self, name: &str) -> Option<usize> {
    return self.state.schema.column_name_to_index.get(name).copied();
  }

  #[inline]
  pub fn column_metadata_by_name(&self, name: &str) -> Option<&ColumnMetadata> {
    return Some(&self.state.schema.column_metadata[self.column_index_by_name(name)?]);
  }

  pub fn primary_key_to_value(&self, pk: String) -> Result<Value, RecordError> {
    // NOTE: loosely parse - will convert STRING to INT/REAL.
    return trailbase_schema::json::parse_string_to_sqlite_value(
      self.state.schema.record_pk_column.column.data_type,
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
  pub fn insert_conflict_resolution_strategy(&self) -> Option<proto::ConflictResolutionStrategy> {
    return self.state.insert_conflict_resolution_strategy;
  }

  pub fn check_table_and_record_level_access<T: SyncConnectionTrait>(
    &self,
    conn: &mut T,
    p: Permission,
    record_id: Option<&Value>,
    request_params: Option<Params>,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    // First check table level access and if present check row-level access based on access rule.
    self.check_table_level_access(p, user)?;

    let Some(access_query) = self.state.cached_access_query(p) else {
      return Ok(());
    };

    let params = RecordAclParams::new(
      p,
      user,
      record_id.cloned(),
      &self.state.schema.column_names,
      request_params,
    )?;

    return Self::check_record_level_access_impl(conn, &access_query, params);
  }

  /// Only checks record-level access. A separate table-level access check is required.
  pub fn check_record_level_access_only<T: SyncConnectionTrait>(
    &self,
    conn: &mut T,
    p: Permission,
    record_id: Option<&Value>,
    request_params: Option<Params>,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    let Some(access_query) = self.state.cached_access_query(p) else {
      return Ok(());
    };

    let params = RecordAclParams::new(
      p,
      user,
      record_id.cloned(),
      &self.state.schema.column_names,
      request_params,
    )?;

    return Self::check_record_level_access_impl(conn, &access_query, params);
  }

  #[inline]
  fn check_record_level_access_impl(
    conn: &mut impl SyncConnectionTrait,
    access_query: &str,
    params: impl trailbase_sqlite::Params,
  ) -> Result<(), RecordError> {
    return match conn.query_row(access_query, params).and_then(|row| {
      row
        .ok_or(trailbase_sqlite::Error::QueryReturnedNoRows)
        .and_then(|r| Ok(r.get::<bool>(0)?))
    }) {
      Ok(allowed) if allowed => Ok(()),
      Ok(_allowed) => Err(RecordError::Forbidden),
      #[cfg_attr(test, allow(unreachable_code))]
      Err(err) => {
        #[cfg(test)]
        panic!("RLA query '{access_query}' failed: {err}");

        log::warn!(
          "RecordApi failing closed. Access query '{access_query}' needs to yield TRUE/FALSE, instead produced error: {err}"
        );

        Err(RecordError::Forbidden)
      }
    };
  }

  /// Check if the given user (if any) can access a record given the request and the operation.
  ///
  /// QUESTION: Could we structure this in a way that we yield less in case there's no read-access
  /// rule, e.g. sync and return (yikes):
  ///   `Result<Option<Box<dyn Future<Output=Result<(), RecordError>>>>, RecordErrorr>`
  #[inline]
  pub(crate) async fn check_record_level_read_access_for_subscriptions(
    &self,
    record: &Arc<EventPayload>,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    // First check table level access and if present check row-level access based on access rule.
    self.check_table_level_access(Permission::Read, user)?;

    let Some(subscription_read_access_query) = self.state.subscription_read_access_query.clone()
    else {
      return Ok(());
    };

    let params = SubscriptionAclParams::new(record, user);

    return self
      .state
      .conn
      .call_reader(move |mut conn| {
        Self::check_record_level_access_impl(&mut conn, &subscription_read_access_query, params)
      })
      .await;
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
}

pub struct RecordApiByName(pub RecordApi);

// Hash only the `id` field
impl Hash for RecordApiByName {
  fn hash<H: Hasher>(&self, state: &mut H) {
    return self.0.api_name().hash(state);
  }
}

// Equality is based only on the `id` field
impl PartialEq for RecordApiByName {
  fn eq(&self, other: &Self) -> bool {
    return self.0.api_name() == other.0.api_name();
  }
}
impl Eq for RecordApiByName {}

impl std::borrow::Borrow<str> for RecordApiByName {
  fn borrow(&self) -> &str {
    return self.0.api_name();
  }
}

impl std::ops::Deref for RecordApiByName {
  type Target = RecordApi;

  fn deref(&self) -> &Self::Target {
    return &self.0;
  }
}

struct SubscriptionAclParams {
  user: Option<[u8; 16]>,
  tenant: Option<[u8; 16]>,
  params: Arc<EventPayload>,
}

impl SubscriptionAclParams {
  fn new(event: &Arc<EventPayload>, user: Option<&User>) -> Self {
    return Self {
      params: event.clone(),
      user: user.map(|u| u.id.into_bytes()),
      tenant: user.and_then(|u| u.tenant.map(|t| t.into_bytes())),
    };
  }
}

impl trailbase_sqlite::Params for SubscriptionAclParams {
  fn bind<S: trailbase_sqlite::Statement>(
    self,
    stmt: &mut S,
  ) -> Result<(), trailbase_sqlite::Error> {
    // NOTE: We only have to provide values for placeholders in
    // `core/templates/subscription_record_access_query.sql`, i.e. we can skip `:__fields`, etc.
    match *self.params {
      EventPayload::Insert { ref record, .. }
      | EventPayload::Update { ref record, .. }
      | EventPayload::Delete { ref record, .. } => {
        for (name, v) in record.iter() {
          // Micro-optimization: can we push the placeholder construction off?
          if let Some(idx) = debug_is_some!(stmt.parameter_index(&named_placeholder(name))?) {
            stmt.bind_parameter(idx, ToSqlProxy::Borrowed(v.into()))?;
          };
        }
      }
      _ => {
        return Err(trailbase_sqlite::Error::Other("not a change param".into()));
      }
    }

    if let Some(idx) = debug_is_some!(stmt.parameter_index(":__user_id")?) {
      if let Some(user) = self.user {
        stmt.bind_parameter(idx, ToSqlProxy::Borrowed(ValueRef::Blob(&user)))?;
      } else {
        stmt.bind_parameter(idx, ToSqlProxy::Borrowed(ValueRef::Null))?;
      }
    }

    if let Some(idx) = debug_is_some!(stmt.parameter_index(":__tenant_id")?) {
      if let Some(tenant) = self.tenant {
        stmt.bind_parameter(idx, ToSqlProxy::Borrowed(ValueRef::Blob(&tenant)))?;
      } else {
        stmt.bind_parameter(idx, ToSqlProxy::Borrowed(ValueRef::Null))?;
      }
    }

    return Ok(());
  }
}

#[derive(Template)]
#[template(
  escape = "none",
  whitespace = "minimize",
  path = "subscription_record_access_query.sql"
)]
struct SubscriptionRecordReadTemplate<'a> {
  read_access_rule: &'a str,
  column_names: Vec<&'a str>,
}

/// Build access query for record reads, deletes and query access.
///
/// Assumes access_rule is an expression: https://www.sqlite.org/syntax/expr.html
fn build_read_delete_schema_query(
  connection_type: ConnectionType,
  qualified_table_name: &QualifiedNameEscaped,
  pk_column_name: &str,
  access_rule: &str,
) -> Arc<str> {
  return match connection_type {
    ConnectionType::Pg => format!(
      "\
      SELECT \
        CAST(({access_rule}) AS INTEGER) \
      FROM \
        (SELECT CAST(:__user_id AS uuid) AS id, CAST(:__tenant_id AS uuid) AS tenant) AS _USER_, \
        (SELECT * FROM {qualified_table_name} WHERE \"{pk_column_name}\" = :__record_id) AS _ROW_ \
    ",
    )
    .into(),
    ConnectionType::Sqlite => format!(
      "\
      SELECT \
        CAST(({access_rule}) AS INTEGER) \
      FROM \
        (SELECT :__user_id AS id, :__tenant_id AS tenant) AS _USER_, \
        (SELECT * FROM {qualified_table_name} WHERE \"{pk_column_name}\" = :__record_id) AS _ROW_ \
    ",
    )
    .into(),
  };
}

#[derive(Template)]
#[template(
  escape = "none",
  whitespace = "minimize",
  path = "create_record_access_query.sql"
)]
struct CreateRecordAccessQueryTemplateSqlite<'a> {
  create_access_rule: &'a str,
  column_metadata: &'a [ColumnMetadata],
}

#[derive(Template)]
#[template(
  escape = "none",
  whitespace = "minimize",
  path = "create_record_access_query_pg.sql"
)]
struct CreateRecordAccessQueryTemplatePg<'a> {
  create_access_rule: &'a str,
  column_metadata: &'a [ColumnMetadata],
}

/// Build access query for record creation.
///
/// Assumes access_rule is an expression: https://www.sqlite.org/syntax/expr.html
#[inline]
fn build_create_access_query(
  connection_type: ConnectionType,
  column_metadata: &[ColumnMetadata],
  create_access_rule: &str,
) -> Result<Arc<str>, String> {
  return Ok(match connection_type {
    ConnectionType::Sqlite => CreateRecordAccessQueryTemplateSqlite {
      create_access_rule,
      column_metadata,
    }
    .render()
    .map_err(|err| err.to_string())?
    .into(),
    ConnectionType::Pg => CreateRecordAccessQueryTemplatePg {
      create_access_rule,
      column_metadata,
    }
    .render()
    .map_err(|err| err.to_string())?
    .into(),
  });
}

#[derive(Template)]
#[template(
  escape = "none",
  whitespace = "minimize",
  path = "update_record_access_query.sql"
)]
struct UpdateRecordAccessQueryTemplateSqlite<'a> {
  update_access_rule: &'a str,
  table_name: &'a QualifiedNameEscaped,
  pk_column_name: &'a str,
  column_metadata: &'a [ColumnMetadata],
}

#[derive(Template)]
#[template(
  escape = "none",
  whitespace = "minimize",
  path = "update_record_access_query_pg.sql"
)]
struct UpdateRecordAccessQueryTemplatePg<'a> {
  update_access_rule: &'a str,
  table_name: &'a QualifiedNameEscaped,
  pk_column_name: &'a str,
  column_metadata: &'a [ColumnMetadata],
}

/// Build access query for record updates.
///
/// Assumes access_rule is an expression: https://www.sqlite.org/syntax/expr.html
#[inline]
fn build_update_access_query(
  connection_type: ConnectionType,
  table_name: &QualifiedNameEscaped,
  column_metadata: &[ColumnMetadata],
  pk_column_name: &str,
  update_access_rule: &str,
) -> Result<Arc<str>, String> {
  return Ok(match connection_type {
    ConnectionType::Sqlite => UpdateRecordAccessQueryTemplateSqlite {
      update_access_rule,
      table_name,
      pk_column_name,
      column_metadata,
    }
    .render()
    .map_err(|err| err.to_string())?
    .into(),
    ConnectionType::Pg => UpdateRecordAccessQueryTemplatePg {
      update_access_rule,
      table_name,
      pk_column_name,
      column_metadata,
    }
    .render()
    .map_err(|err| err.to_string())?
    .into(),
  });
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

fn filter_excluded_columns(
  config: &proto::RecordApiConfig,
  column_metadata: &[ColumnMetadata],
) -> Vec<ColumnMetadata> {
  if config.excluded_columns.is_empty() {
    return column_metadata.to_vec();
  }

  return column_metadata
    .iter()
    .filter_map(|meta| {
      if config.excluded_columns.contains(&meta.column.name) {
        return None;
      }
      return Some(meta.clone());
    })
    .collect();
}

#[inline]
fn assert_name(config: &proto::RecordApiConfig, name: &QualifiedName) {
  // QUESTION: Should this be disabled in prod? This can only trigger during start and config
  // reload.
  let config_table_name =
    match QualifiedName::parse(config.table_name.as_deref().unwrap_or_default()) {
      Ok(name) => name,
      Err(err) => {
        panic!("Failed to parse '{:?}': {err}", config.table_name);
      }
    };

  debug_assert_eq!(
    config_table_name, *name,
    "Expected: {name}, got: {config_table_name}"
  );
}

#[inline]
fn strings_to_json_array<'a, I>(iter: I) -> String
where
  I: Iterator<Item = &'a str>,
{
  let mut out = String::with_capacity(256);

  out.push('[');
  for (i, s) in iter.enumerate() {
    if i > 0 {
      out.push(',');
    }
    write_json_string(&mut out, s);
  }
  out.push(']');

  return out;
}

#[inline]
fn write_json_string(out: &mut String, s: &str) {
  out.push('"');
  for c in s.chars() {
    match c {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      '\u{08}' => out.push_str("\\b"),
      '\u{0C}' => out.push_str("\\f"),
      // Control characters must be escaped as \u00XX.
      c if (c as u32) < 0x20 => {
        out.push_str(&format!("\\u{:04x}", c as u32));
      }
      c => out.push(c),
    }
  }
  out.push('"');
}

#[cfg(test)]
mod tests {
  use trailbase_schema::db::parse_sql::{Bump, parse_into_statement};
  use trailbase_schema::db::sqlite::{Column, ColumnAffinityType, ColumnDataType, QualifiedName};

  use super::*;
  use crate::{config::proto::PermissionFlag, records::Permission};

  fn sanitize_template(template: &str) {
    let allocator = Bump::new();
    let r = parse_into_statement(&allocator, template);
    assert!(r.is_ok(), "{template}: {:?}", r.err());
    assert!(!template.contains("   "), "{template}");
    assert!(!template.contains("\n\n"), "{template}");
  }

  fn index_metadata() -> ColumnMetadata {
    return ColumnMetadata {
      index: 0,
      column: Column {
        name: "index".to_string(),
        type_name: "uuid".to_string(),
        data_type: ColumnDataType::Blob,
        affinity_type: ColumnAffinityType::Blob,
        options: vec![],
      },
      json: None,
      is_file: false,
      is_geometry: false,
      is_fk: false,
    };
  }

  #[test]
  fn test_create_record_access_query_template() {
    {
      let query = CreateRecordAccessQueryTemplateSqlite {
        create_access_rule: "_USER_.id = X'05'",
        column_metadata: &[],
      }
      .render()
      .unwrap();

      sanitize_template(&query);

      let query = CreateRecordAccessQueryTemplatePg {
        create_access_rule: "_USER_.id = '\\x05'",
        column_metadata: &[],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = CreateRecordAccessQueryTemplateSqlite {
        create_access_rule: r#"_USER_.id = X'05' AND "index" = 'secret'"#,
        column_metadata: &vec![index_metadata()],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }
  }

  #[test]
  fn test_update_record_access_query_template() {
    {
      let query = UpdateRecordAccessQueryTemplateSqlite {
        update_access_rule: r#"_USER_.id = X'05' AND _ROW_."index" = 'secret'"#,
        table_name: &QualifiedName::parse("table").unwrap().into(),
        pk_column_name: "index",
        column_metadata: &[],
      }
      .render()
      .unwrap();

      sanitize_template(&query);

      let query = UpdateRecordAccessQueryTemplatePg {
        update_access_rule: r#"_USER_.id = X'05' AND _ROW_."index" = 'secret'"#,
        table_name: &QualifiedName::parse("table").unwrap().into(),
        pk_column_name: "index",
        column_metadata: &[],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = UpdateRecordAccessQueryTemplateSqlite {
        update_access_rule: r#"_USER_.id = X'05' AND _ROW_."index" = _REQ_."index""#,
        table_name: &QualifiedName::parse("table").unwrap().into(),
        pk_column_name: "index",
        column_metadata: &vec![index_metadata()],
      }
      .render()
      .unwrap();

      sanitize_template(&query);

      let query = UpdateRecordAccessQueryTemplatePg {
        update_access_rule: r#"_USER_.id = X'05' AND _ROW_."index" = _REQ_."index""#,
        table_name: &QualifiedName::parse("table").unwrap().into(),
        pk_column_name: "index",
        column_metadata: &vec![index_metadata()],
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
