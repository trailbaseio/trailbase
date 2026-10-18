use trailbase_schema::db::QualifiedNameEscaped;
use trailbase_sqlite::{ConnectionType, SyncConnectionTrait, Value};

use crate::auth::User;
use crate::records::Permission;
use crate::records::error::RecordError;
use crate::records::queries::WriteQueryResult;
use crate::records::record_api::RecordApi;
use crate::util::row_id_column;

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
    return self.api.check_record_level_access_only(
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
