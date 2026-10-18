use askama::Template;
use object_store::ObjectStore;
use std::sync::Arc;
use trailbase_schema::db::QualifiedNameEscaped;
use trailbase_schema::db::metadata::ColumnMetadata;
use trailbase_sqlite::{Connection, ConnectionType, SyncConnectionTrait, Value};

use crate::auth::User;
use crate::config::proto;
use crate::records::Permission;
use crate::records::error::RecordError;
use crate::records::files::{FileManager, delete_files_marked_for_deletion};
use crate::records::params::{
  ArcInsertParams, ArcUpdateParams, CowArc, FileMetadataContents, InsertParams, NamedParam, Params,
  UpdateParams,
};
use crate::records::record_api::RecordApi;
use crate::util::row_id_column;

#[derive(Debug)]
pub struct InsertOrReplaceQuery {
  query: String,
  params: InsertParams,
}

impl InsertOrReplaceQuery {
  pub fn new(
    connection_type: ConnectionType,
    table_name: &QualifiedNameEscaped,
    column_metadata: &[ColumnMetadata],
    pk_column_name: &str,
    conflict_resolution: proto::ConflictResolutionStrategy,
    params: InsertParams,
  ) -> Result<Self, RecordError> {
    let row_id_column = row_id_column(connection_type);
    let query = CreateOrReplaceRecordQueryTemplate {
      table_name,
      params: &params.params,
      pk_column_name,
      column_metadata: match conflict_resolution {
        proto::ConflictResolutionStrategy::Replace => column_metadata,
        proto::ConflictResolutionStrategy::Ignore => &[],
        proto::ConflictResolutionStrategy::Abort | proto::ConflictResolutionStrategy::Undefined => {
          &[]
        }
      },
      ignore_conflict: matches!(
        conflict_resolution,
        proto::ConflictResolutionStrategy::Ignore
      ),
      returning: &[row_id_column, pk_column_name],
    }
    .render()
    .map_err(|err| RecordError::Internal(err.into()))?;

    return Ok(Self { query, params });
  }

  pub fn apply(
    &self,
    conn: &mut impl SyncConnectionTrait,
  ) -> Result<WriteQueryResult, trailbase_sqlite::Error> {
    return if let Some(mut row) =
      conn.query_row(&self.query, ArcInsertParams(CowArc::Borrowed(&self.params)))?
    {
      Ok(WriteQueryResult {
        rowid: row.get(0)?,
        pk_value: Some(row.consume_value(1)?),
      })
    } else {
      Err(trailbase_sqlite::Error::QueryReturnedNoRows)
    };
  }
}

pub struct InsertOrReplaceRecordApiQuery {
  api: RecordApi,
  query: InsertOrReplaceQuery,
}

impl InsertOrReplaceRecordApiQuery {
  pub fn new(api: RecordApi, params: InsertParams) -> Result<Self, RecordError> {
    let conflict_resolution = api
      .insert_conflict_resolution_strategy()
      .unwrap_or(proto::ConflictResolutionStrategy::Undefined);

    return Ok(Self {
      query: InsertOrReplaceQuery::new(
        api.conn().connection_type(),
        api.table_name(),
        api.columns(),
        &api.record_pk_column().column.name,
        conflict_resolution,
        params,
      )?,
      api,
    });
  }

  pub fn check_table_level_access(&self, user: Option<&User>) -> Result<(), RecordError> {
    return self.api.check_table_level_access(Permission::Create, user);
  }

  pub fn check_record_level_access(
    &self,
    conn: &mut impl SyncConnectionTrait,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    return self.api.sync_eager_check_record_level_access(
      conn,
      Permission::Create,
      None,
      Some(Params::Insert(CowArc::Borrowed(&self.query.params))),
      user,
    );
  }

  pub fn apply(
    &self,
    conn: &mut impl SyncConnectionTrait,
  ) -> Result<WriteQueryResult, trailbase_sqlite::Error> {
    return self.query.apply(conn);
  }
}

#[derive(Debug)]
pub struct UpdateQuery {
  query: String,
  params: UpdateParams,
}

impl UpdateQuery {
  pub fn new(
    connection_type: ConnectionType,
    table_name: &QualifiedNameEscaped,
    params: UpdateParams,
  ) -> Result<Self, RecordError> {
    let query = UpdateRecordQueryTemplate {
      table_name,
      params: &params.params,
      pk_column_name: &params.pk_column_name,
      returning: Some(row_id_column(connection_type)),
    }
    .render()
    .map_err(|err| RecordError::Internal(err.into()))?;

    return Ok(UpdateQuery { query, params });
  }

  pub fn apply(
    &self,
    conn: &mut impl SyncConnectionTrait,
  ) -> Result<WriteQueryResult, trailbase_sqlite::Error> {
    return if let Some(row) =
      conn.query_row(&self.query, ArcUpdateParams(CowArc::Borrowed(&self.params)))?
    {
      Ok(WriteQueryResult {
        rowid: row.get(0)?,
        pk_value: None,
      })
    } else {
      Err(trailbase_sqlite::Error::QueryReturnedNoRows)
    };
  }
}

pub struct UpdateRecordApiQuery {
  api: RecordApi,
  query: UpdateQuery,
}

impl UpdateRecordApiQuery {
  pub fn new(api: RecordApi, params: UpdateParams) -> Result<Self, RecordError> {
    return Ok(Self {
      query: UpdateQuery::new(api.conn().connection_type(), api.table_name(), params)?,
      api,
    });
  }

  pub fn pk_value(&self) -> Value {
    return self.query.params.pk_column_value.clone();
  }

  pub fn check_table_level_access(&self, user: Option<&User>) -> Result<(), RecordError> {
    return self.api.check_table_level_access(Permission::Update, user);
  }

  pub fn check_record_level_access(
    &self,
    conn: &mut impl SyncConnectionTrait,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    return self.api.sync_eager_check_record_level_access(
      conn,
      Permission::Update,
      Some(&self.query.params.pk_column_value),
      Some(Params::Update(CowArc::Borrowed(&self.query.params))),
      user,
    );
  }

  pub fn apply(
    &self,
    conn: &mut impl SyncConnectionTrait,
  ) -> Result<WriteQueryResult, trailbase_sqlite::Error> {
    return self.query.apply(conn);
  }
}

#[derive(Debug)]
pub struct DeleteQuery {
  query: String,
  pk_value: Value,
}

impl DeleteQuery {
  pub fn new(
    connection_type: ConnectionType,
    table_name: &QualifiedNameEscaped,
    pk_column_name: &str,
    pk_value: Value,
  ) -> Result<Self, RecordError> {
    return Ok(Self {
      query: format!(
        r#"DELETE FROM {table_name} WHERE "{pk_column_name}" = $1 RETURNING {row_id}"#,
        row_id = row_id_column(connection_type)
      ),
      pk_value,
    });
  }

  pub fn apply(
    &self,
    conn: &mut impl SyncConnectionTrait,
  ) -> Result<WriteQueryResult, trailbase_sqlite::Error> {
    return if let Some(row) = conn.query_row(&self.query, (&self.pk_value,))? {
      Ok(WriteQueryResult {
        rowid: row.get(0)?,
        pk_value: None,
      })
    } else {
      Err(trailbase_sqlite::Error::QueryReturnedNoRows)
    };
  }
}

pub struct DeleteRecordApiQuery {
  api: RecordApi,
  query: DeleteQuery,
}

impl DeleteRecordApiQuery {
  pub fn new(api: RecordApi, record_id: Value) -> Result<Self, RecordError> {
    return Ok(Self {
      query: DeleteQuery::new(
        api.conn().connection_type(),
        api.table_name(),
        &api.record_pk_column().column.name,
        record_id,
      )?,
      api,
    });
  }

  pub fn pk_value(&self) -> Value {
    return self.query.pk_value.clone();
  }

  pub fn check_table_level_access(&self, user: Option<&User>) -> Result<(), RecordError> {
    return self.api.check_table_level_access(Permission::Delete, user);
  }

  pub fn check_record_level_access(
    &self,
    conn: &mut impl SyncConnectionTrait,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    return self.api.sync_eager_check_record_level_access(
      conn,
      Permission::Delete,
      Some(&self.query.pk_value),
      None,
      user,
    );
  }

  pub fn apply(
    &self,
    conn: &mut impl SyncConnectionTrait,
  ) -> Result<WriteQueryResult, trailbase_sqlite::Error> {
    return self.query.apply(conn);
  }
}

pub enum WriteQuery {
  Insert(InsertOrReplaceRecordApiQuery),
  Update(UpdateRecordApiQuery),
  Delete { query: String, pk_value: Value },
}

pub struct WriteQueryResult {
  /// Row-id (SQLite) or ctid (Postgres) of affected record. Always present.
  pub rowid: i64,
  /// Primary key value. Only present on inserts.
  pub pk_value: Option<Value>,
}

impl WriteQuery {
  pub fn new_insert_or_replace(api: RecordApi, params: InsertParams) -> Result<Self, RecordError> {
    return Ok(Self::Insert(InsertOrReplaceRecordApiQuery::new(
      api, params,
    )?));
  }

  pub fn new_update(api: RecordApi, params: UpdateParams) -> Result<Self, RecordError> {
    return Ok(Self::Update(UpdateRecordApiQuery::new(api, params)?));
  }

  pub fn new_delete(
    connection_type: ConnectionType,
    table_name: &QualifiedNameEscaped,
    pk_column_name: &str,
    pk_value: Value,
  ) -> Result<Self, RecordError> {
    return Ok(Self::Delete {
      query: format!(
        r#"DELETE FROM {table_name} WHERE "{pk_column_name}" = $1 RETURNING {row_id}"#,
        row_id = row_id_column(connection_type)
      ),
      pk_value,
    });
  }

  pub(super) fn apply_sync(
    self,
    conn: &mut impl SyncConnectionTrait,
  ) -> Result<WriteQueryResult, trailbase_sqlite::Error> {
    return match self {
      Self::Insert(query) => query.apply(conn),
      Self::Update(query) => query.apply(conn),
      Self::Delete { query, pk_value } => {
        if let Some(row) = conn.query_row(query, (pk_value,))? {
          Ok(WriteQueryResult {
            rowid: row.get(0)?,
            pk_value: None,
          })
        } else {
          Err(trailbase_sqlite::Error::QueryReturnedNoRows)
        }
      }
    };
  }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_insert_or_replace_query(
  conn: &Connection,
  objectstore: &Arc<dyn ObjectStore>,
  table_name: &QualifiedNameEscaped,
  column_metadata: &[ColumnMetadata],
  conflict_resolution: proto::ConflictResolutionStrategy,
  return_column_name: &str,
  params: InsertParams,
  files: FileMetadataContents,
) -> Result<Value, RecordError> {
  let query = InsertOrReplaceQuery::new(
    conn.connection_type(),
    table_name,
    column_metadata,
    return_column_name,
    conflict_resolution,
    params,
  )?;

  // We're storing any files to the object store first to make sure the DB entry is valid right
  // after commit and not racily pointing to soon-to-be-written files.
  let file_manager = if files.is_empty() {
    None
  } else {
    Some(FileManager::write(objectstore, files).await?)
  };

  let WriteQueryResult { rowid, pk_value } = conn
    .call_writer(move |mut conn| query.apply(&mut conn))
    .await?;

  let Some(return_value) = pk_value else {
    return Err(RecordError::Internal("missing pk".into()));
  };

  // Successful write, do not cleanup written files unless old ones were overridden.
  if let Some(mut file_manager) = file_manager {
    file_manager.release();

    if conflict_resolution == proto::ConflictResolutionStrategy::Replace {
      delete_files_marked_for_deletion(conn, objectstore, table_name, &[rowid])
        .await
        .map_err(|err| RecordError::Internal(err.into()))?;
    }
  }

  return Ok(return_value);
}

pub(crate) async fn run_update_query(
  conn: &Connection,
  objectstore: &Arc<dyn ObjectStore>,
  table_name: &QualifiedNameEscaped,
  params: UpdateParams,
  files: FileMetadataContents,
) -> Result<(), RecordError> {
  let query = UpdateQuery::new(conn.connection_type(), table_name, params)?;

  // We're storing any files to the object store first to make sure the DB entry is valid right
  // after commit and not racily pointing to soon-to-be-written files.
  let file_manager = if files.is_empty() {
    None
  } else {
    Some(FileManager::write(objectstore, files).await?)
  };

  let WriteQueryResult { rowid, pk_value: _ } = conn
    .call_writer(move |mut conn| query.apply(&mut conn))
    .await?;

  // Successful write, do not cleanup written files.
  if let Some(mut file_manager) = file_manager {
    file_manager.release();
    delete_files_marked_for_deletion(conn, objectstore, table_name, &[rowid])
      .await
      .map_err(|err| RecordError::Internal(err.into()))?;
  }

  return Ok(());
}

pub(crate) async fn run_delete_query(
  conn: &Connection,
  objectstore: &Arc<dyn ObjectStore>,
  table_name: &QualifiedNameEscaped,
  pk_column: &str,
  pk_value: Value,
  has_file_columns: bool,
) -> Result<i64, RecordError> {
  let query = DeleteQuery::new(conn.connection_type(), table_name, pk_column, pk_value)?;

  let WriteQueryResult { rowid, pk_value: _ } = conn
    .call_writer(move |mut conn| query.apply(&mut conn))
    .await?;

  if has_file_columns {
    delete_files_marked_for_deletion(conn, objectstore, table_name, &[rowid])
      .await
      .map_err(|err| RecordError::Internal(err.into()))?;
  }

  return Ok(rowid);
}

#[derive(Template)]
#[template(escape = "none", path = "update_record_query.sql")]
struct UpdateRecordQueryTemplate<'a> {
  table_name: &'a QualifiedNameEscaped,
  params: &'a [NamedParam],
  pk_column_name: &'a str,
  returning: Option<&'a str>,
}

#[derive(Template)]
#[template(escape = "none", path = "create_or_replace_record_query.sql")]
struct CreateOrReplaceRecordQueryTemplate<'a> {
  table_name: &'a QualifiedNameEscaped,
  params: &'a [NamedParam],
  pk_column_name: &'a str,
  column_metadata: &'a [ColumnMetadata],
  ignore_conflict: bool,
  returning: &'a [&'a str],
}

#[cfg(test)]
mod tests {
  use super::*;

  use serde_json::json;
  use trailbase_schema::db::parse_sql::{Bump, parse_into_statement};
  use trailbase_schema::db::sqlite::Table;
  use trailbase_schema::db::{QualifiedName, QualifiedNameEscaped};

  use crate::config::proto;
  use crate::records::test_utils::json_row_from_value;
  use crate::records::write_queries::InsertOrReplaceQuery;
  use crate::schema_metadata::TableMetadata;

  fn sanitize_template(template: &str) {
    let allocator = Bump::new();
    let r = parse_into_statement(&allocator, template);
    assert!(r.is_ok(), "{template}: {:?}", r.err());
    assert!(!template.contains("\n\n"), "{template}");
    assert!(!template.contains("   "), "{template}");
  }

  #[test]
  fn test_create_record_template() {
    {
      let query = CreateOrReplaceRecordQueryTemplate {
        table_name: &QualifiedName::parse("table").unwrap().into(),
        params: &[
          NamedParam {
            column_index: 0,
            column_name: "index".into(),
            placeholder: ":index".into(),
            value: Value::Null,
          },
          NamedParam {
            column_index: 1,
            column_name: "trigger".into(),
            placeholder: ":trigger".into(),
            value: Value::Null,
          },
        ],
        pk_column_name: "index",
        column_metadata: &[],
        ignore_conflict: false,
        returning: &["index"],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = CreateOrReplaceRecordQueryTemplate {
        table_name: &QualifiedName {
          name: "table".to_string(),
          database_schema: Some("db".to_string()),
        }
        .into(),
        params: &[],
        pk_column_name: "id",
        column_metadata: &[],
        ignore_conflict: false,
        returning: &["*"],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = CreateOrReplaceRecordQueryTemplate {
        table_name: &QualifiedName::parse("table").unwrap().into(),
        params: &[NamedParam {
          column_index: 0,
          column_name: "index".into(),
          placeholder: ":index".into(),
          value: Value::Null,
        }],
        pk_column_name: "index",
        column_metadata: &[],
        ignore_conflict: true,
        returning: &[],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }
  }

  #[test]
  fn test_empty_insert_params() {
    let table_name = "test_table".to_string();

    let allocator = Bump::new();
    let table: Table = (&parse_into_statement(
      &allocator,
      &format!(
        "CREATE TABLE {table_name} (
          id      INTEGER PRIMARY KEY,
          data    TEXT
        ) STRICT;"
      ),
    )
    .unwrap()
    .unwrap())
      .try_into()
      .unwrap();

    let registry =
      trailbase_schema::json_schema::registry::build_json_schema_registry(vec![]).unwrap();
    let metadata = TableMetadata::new(&registry, table.clone(), &[table]).unwrap();
    let qualified_name = QualifiedNameEscaped::new(&QualifiedName {
      name: table_name.clone(),
      database_schema: None,
    });

    struct Input {
      input: serde_json::Value,
      expected_params: usize,
    }

    for i in [
      Input {
        input: json!({}),
        expected_params: 0,
      },
      Input {
        input: json!({"unknown_column": 5}),
        expected_params: 0,
      },
      Input {
        input: json!({"data": "test"}),
        expected_params: 1,
      },
      Input {
        input: json!({"id": 5, "data": "test"}),
        expected_params: 2,
      },
    ] {
      let (params, _files) = InsertParams::new(
        &metadata,
        &registry,
        json_row_from_value(i.input).unwrap(),
        None,
      )
      .unwrap();

      assert_eq!(i.expected_params, params.params.len());

      let query = InsertOrReplaceQuery::new(
        ConnectionType::Sqlite,
        &qualified_name,
        &metadata.column_metadata,
        "id",
        proto::ConflictResolutionStrategy::Replace,
        params,
      )
      .unwrap();

      // Make sure it's a valid query
      assert!(
        parse_into_statement(&allocator, &query.query).is_ok(),
        "{query:?}"
      )
    }
  }
}
