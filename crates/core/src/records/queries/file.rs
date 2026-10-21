use trailbase_schema::db::QualifiedNameEscaped;
use trailbase_schema::db::metadata::ColumnMetadata;
use trailbase_sqlite::{SyncConnectionTrait, Value};

use crate::auth::User;
use crate::records::Permission;
use crate::records::error::RecordError;
use crate::records::record_api::RecordApi;
use crate::schema_metadata::JsonColumnMetadata;

pub struct ReadFileQuery {
  query: String,
  pk_value: Value,
}

impl ReadFileQuery {
  pub fn new(
    expected_schema_name: &str,
    table_name: &QualifiedNameEscaped,
    file_column_meta: &ColumnMetadata,
    pk_column_name: &str,
    pk_value: Value,
  ) -> Result<Self, RecordError> {
    let Some(JsonColumnMetadata::SchemaName(name)) = &file_column_meta.json else {
      return Err(RecordError::BadRequest("not a file column"));
    };

    if name != expected_schema_name {
      return Err(RecordError::BadRequest("column mismatch"));
    }

    let query = format!(
      r#"SELECT "{column_name}" FROM {table_name} WHERE "{pk_column_name}" = $1"#,
      column_name = file_column_meta.column.name,
    );

    return Ok(Self { query, pk_value });
  }

  /// Returns JSON file metadata string.
  pub fn read(&self, conn: &mut impl SyncConnectionTrait) -> Result<String, RecordError> {
    return Ok(
      conn
        .query_row(&self.query, (&self.pk_value,))?
        .ok_or(RecordError::RecordNotFound)?
        .get(0)?,
    );
  }
}

pub struct ReadFileRecordApiQuery {
  api: RecordApi,
  query: ReadFileQuery,
}

impl ReadFileRecordApiQuery {
  pub fn new_single_file_upload(
    api: RecordApi,
    file_column_meta: &ColumnMetadata,
    record_id: Value,
  ) -> Result<Self, RecordError> {
    return Ok(Self {
      query: ReadFileQuery::new(
        "std.FileUpload",
        api.table_name(),
        file_column_meta,
        &api.record_pk_column().column.name,
        record_id,
      )?,
      api,
    });
  }

  pub fn new_file_uploads(
    api: RecordApi,
    file_column_meta: &ColumnMetadata,
    record_id: Value,
  ) -> Result<Self, RecordError> {
    return Ok(Self {
      query: ReadFileQuery::new(
        "std.FileUploads",
        api.table_name(),
        file_column_meta,
        &api.record_pk_column().column.name,
        record_id,
      )?,
      api,
    });
  }

  pub fn check_table_level_access(&self, user: Option<&User>) -> Result<(), RecordError> {
    return self.api.check_table_level_access(Permission::Read, user);
  }

  pub fn check_record_level_access(
    &self,
    conn: &mut impl SyncConnectionTrait,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    return self.api.check_record_level_access_only(
      conn,
      Permission::Read,
      Some(&self.query.pk_value),
      None,
      user,
    );
  }

  /// Returns JSON file metadata string.
  pub fn read(&self, conn: &mut impl SyncConnectionTrait) -> Result<String, RecordError> {
    return self.query.read(conn);
  }
}
