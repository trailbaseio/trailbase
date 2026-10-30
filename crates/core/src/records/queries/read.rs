use askama::Template;
use trailbase_schema::db::QualifiedNameEscaped;
use trailbase_sqlite::{SyncConnectionTrait, Value};

use crate::auth::User;
use crate::records::Permission;
use crate::records::error::RecordError;
use crate::records::expand::ExpandedTable;
use crate::records::record_api::RecordApi;

pub struct ReadRecordApiQuery {
  api: RecordApi,
  query: String,
  record_id: Value,
}

impl ReadRecordApiQuery {
  pub fn new(
    api: RecordApi,
    record_id: Value,
    expanded_tables: Option<&[ExpandedTable]>,
  ) -> Result<Self, RecordError> {
    let query = ReadRecordQueryTemplate {
      table_name: api.table_name(),
      column_names: &api
        .columns()
        .iter()
        .map(|meta| meta.column.name.as_str())
        .collect::<Vec<_>>(),
      pk_column_name: &api.record_pk_column().column.name,
      expanded_tables: expanded_tables.unwrap_or_default(),
    }
    .render()
    .map_err(|err| RecordError::Internal(err.into()))?;

    return Ok(Self {
      api,
      query,
      record_id,
    });
  }

  pub fn check_record_level_access(
    &self,
    conn: &mut impl SyncConnectionTrait,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    return self.api.check_record_level_access_only(
      conn,
      Permission::Read,
      Some(&self.record_id),
      None,
      user,
    );
  }

  pub fn read(
    &self,
    conn: &mut impl SyncConnectionTrait,
  ) -> Result<trailbase_sqlite::Row, RecordError> {
    return conn
      .query_row(&self.query, (&self.record_id,))?
      .ok_or(RecordError::RecordNotFound);
  }
}

#[derive(Template)]
#[template(escape = "none", path = "read_record_query.sql")]
struct ReadRecordQueryTemplate<'a> {
  table_name: &'a QualifiedNameEscaped,
  column_names: &'a [&'a str],
  pk_column_name: &'a str,
  expanded_tables: &'a [ExpandedTable<'a>],
}
