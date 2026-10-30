use askama::Template;
use trailbase_schema::db::QualifiedNameEscaped;
use trailbase_sqlite::{ConnectionType, SyncConnectionTrait, Value};

use crate::auth::User;
use crate::records::Permission;
use crate::records::error::RecordError;
use crate::records::params::{NamedParam, Params, UpdateParams, UpdateParamsRef};
use crate::records::queries::WriteQueryResult;
use crate::records::record_api::RecordApi;
use crate::util::row_id_column;

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
    return if let Some(row) = conn.query_row(&self.query, UpdateParamsRef(&self.params))? {
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
      query: UpdateQuery::new(api.connection_type(), api.table_name(), params)?,
      api,
    });
  }

  pub fn pk_value(&self) -> Value {
    return self.query.params.pk_column_value.clone();
  }

  pub fn check_record_level_access(
    &self,
    conn: &mut impl SyncConnectionTrait,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    return self.api.check_record_level_access_only(
      conn,
      Permission::Update,
      Some(&self.query.params.pk_column_value),
      Some(Params::Update(&self.query.params)),
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

#[derive(Template)]
#[template(escape = "none", path = "update_record_query.sql")]
struct UpdateRecordQueryTemplate<'a> {
  table_name: &'a QualifiedNameEscaped,
  params: &'a [NamedParam],
  pk_column_name: &'a str,
  returning: Option<&'a str>,
}
