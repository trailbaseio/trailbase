use itertools::Itertools;
use log::*;
use object_store::ObjectStore;
use std::borrow::Cow;
use std::sync::Arc;
use trailbase_sqlite::schema::{FileUpload, FileUploads};
use trailbase_sqlite::{NamedParams, Params as _, Value};

use crate::config::proto::ConflictResolutionStrategy;
use crate::records::error::RecordError;
use crate::records::files::delete_files_in_row;
use crate::records::params::{FileMetadataContents, Params};
use crate::schema::{Column, ColumnOption};
use crate::table_metadata::{
  ColumnMetadata, JsonColumnMetadata, TableMetadata, TableMetadataCache,
};
use crate::AppState;

#[derive(Debug, Clone, thiserror::Error)]
pub enum QueryError {
  #[error("Precondition error: {0}")]
  Precondition(&'static str),
  #[error("Sql error: {0}")]
  Sql(Arc<rusqlite::Error>),
  #[error("FromSql error: {0}")]
  FromSql(Arc<rusqlite::types::FromSqlError>),
  #[error("Tokio Rusqlite error: {0}")]
  TokioRusqlite(Arc<trailbase_sqlite::Error>),
  #[error("Json serialization error: {0}")]
  JsonSerialization(Arc<serde_json::Error>),
  #[error("ObjectStore error: {0}")]
  Storage(Arc<object_store::Error>),
  #[error("File error: {0}")]
  File(Arc<crate::records::files::FileError>),
  #[error("Not found")]
  NotFound,
}

impl From<serde_json::Error> for QueryError {
  fn from(err: serde_json::Error) -> Self {
    return Self::JsonSerialization(err.into());
  }
}

impl From<trailbase_sqlite::Error> for QueryError {
  fn from(err: trailbase_sqlite::Error) -> Self {
    return Self::TokioRusqlite(err.into());
  }
}

impl From<rusqlite::types::FromSqlError> for QueryError {
  fn from(err: rusqlite::types::FromSqlError) -> Self {
    return Self::FromSql(err.into());
  }
}

impl From<object_store::Error> for QueryError {
  fn from(err: object_store::Error) -> Self {
    return Self::Storage(err.into());
  }
}

impl From<crate::records::files::FileError> for QueryError {
  fn from(err: crate::records::files::FileError) -> Self {
    return Self::File(err.into());
  }
}

pub(crate) struct Expansions {
  /// Contains the indexes on where to cut the resulting Row.
  ///
  /// The joins will lead to a row schema that looks something like:
  ///   (root_table..., foreign_table0..., foreign_table1...).
  pub indexes: Vec<(usize, Arc<TableMetadata>)>,
  /// The actual join statements.
  pub joins: Vec<String>,

  /// Select clauses in case the joins are aliased, i.e. a `prefix` is given.
  pub selects: Option<Vec<String>>,
}

impl Expansions {
  pub(crate) fn build<T: AsRef<str>>(
    table_metadata: &TableMetadataCache,
    table_name: &str,
    expand: &[T],
    prefix: Option<&str>,
  ) -> Result<Expansions, RecordError> {
    let Some(root_table) = table_metadata.get(table_name) else {
      return Err(RecordError::ApiRequiresTable);
    };

    let mut joins = vec![];
    let mut indexes = vec![(root_table.schema.columns.len(), root_table.clone())];

    for (idx, col_name) in expand.iter().enumerate() {
      let col_name = col_name.as_ref();
      if col_name.is_empty() {
        continue;
      }
      let Some((column, _col_metadata)) = root_table.column_by_name(col_name) else {
        return Err(RecordError::ApiRequiresTable);
      };

      let Some(ColumnOption::ForeignKey {
        foreign_table: foreign_table_name,
        referred_columns: _,
        ..
      }) = column
        .options
        .iter()
        .find_or_first(|o| matches!(o, ColumnOption::ForeignKey { .. }))
      else {
        return Err(RecordError::ApiRequiresTable);
      };

      let Some(foreign_table) = table_metadata.get(foreign_table_name) else {
        return Err(RecordError::ApiRequiresTable);
      };

      let Some(foreign_pk_column_idx) = foreign_table.record_pk_column else {
        return Err(RecordError::ApiRequiresTable);
      };

      let foreign_pk_column = &foreign_table.schema.columns[foreign_pk_column_idx].name;

      // TODO: Check that `referred_columns` and foreign_pk_column are the same. It's already
      // validated as part of config validation.

      joins.push(if let Some(ref prefix) =  prefix {
        format!(r#"LEFT JOIN "{foreign_table_name}" AS F{idx} ON {prefix}."{col_name}" = F{idx}."{foreign_pk_column}""#)
      } else {
        format!(r#"LEFT JOIN "{foreign_table_name}" AS F{idx} ON "{col_name}" = F{idx}."{foreign_pk_column}""#)
      });
      indexes.push((foreign_table.schema.columns.len(), foreign_table));
    }

    let selects = if prefix.is_none() {
      None
    } else {
      Some(
        (0..joins.len())
          .map(|idx| format!("F{idx}.*"))
          .collect::<Vec<_>>(),
      )
    };

    return Ok(Expansions {
      indexes,
      joins,
      selects,
    });
  }
}

pub(crate) struct SelectQueryBuilder;

impl SelectQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &str,
    pk_column: &str,
    pk_value: Value,
  ) -> Result<Option<trailbase_sqlite::Row>, trailbase_sqlite::Error> {
    return state
      .conn()
      .query_row(
        &format!(r#"SELECT * FROM "{table_name}" WHERE "{pk_column}" = $1"#),
        [pk_value],
      )
      .await;
  }

  pub(crate) async fn run_expanded(
    state: &AppState,
    table_name: &str,
    pk_column: &str,
    pk_value: Value,
    expand: &[&str],
  ) -> Result<Vec<(Arc<TableMetadata>, trailbase_sqlite::Row)>, RecordError> {
    let table_metadata = state.table_metadata();
    let Expansions {
      indexes,
      joins,
      selects,
    } = Expansions::build(table_metadata, table_name, expand, None)?;

    assert!(selects.is_none());

    let sql = format!(
      r#"SELECT * FROM "{table_name}" AS R {} WHERE R.{pk_column} = $1"#,
      joins.join(" ")
    );

    let Some(row) = state.conn().query_row(&sql, [pk_value]).await? else {
      return Ok(vec![]);
    };

    let mut curr = row;
    let mut result = Vec::with_capacity(indexes.len());
    for (idx, metadata) in indexes {
      let next = curr.split_off(idx);
      result.push((metadata, curr));
      curr = next;
    }

    return Ok(result);
  }
}

pub(crate) struct GetFileQueryBuilder;

impl GetFileQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &str,
    file_column: (&Column, &ColumnMetadata),
    pk_column: &str,
    pk_value: Value,
  ) -> Result<FileUpload, QueryError> {
    return match &file_column.1.json {
      Some(JsonColumnMetadata::SchemaName(name)) if name == "std.FileUpload" => {
        let column_name = &file_column.0.name;

        let Some(row) = state
          .conn()
          .query_row(
            &format!(r#"SELECT "{column_name}" FROM "{table_name}" WHERE "{pk_column}" = $1"#),
            [pk_value],
          )
          .await?
        else {
          return Err(QueryError::NotFound);
        };

        let json: String = row.get(0)?;
        let file_upload: FileUpload = serde_json::from_str(&json)?;
        Ok(file_upload)
      }
      _ => Err(QueryError::Precondition("Not a file")),
    };
  }
}

pub(crate) struct GetFilesQueryBuilder;

impl GetFilesQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    table_name: &str,
    file_column: (&Column, &ColumnMetadata),
    pk_column: &str,
    pk_value: Value,
  ) -> Result<FileUploads, QueryError> {
    return match &file_column.1.json {
      Some(JsonColumnMetadata::SchemaName(name)) if name == "std.FileUploads" => {
        let column_name = &file_column.0.name;

        let Some(row) = state
          .conn()
          .query_row(
            &format!(r#"SELECT "{column_name}" FROM "{table_name}" WHERE "{pk_column}" = $1"#),
            [pk_value],
          )
          .await?
        else {
          return Err(QueryError::NotFound);
        };

        let contents: String = row.get(0)?;
        let file_uploads: FileUploads = serde_json::from_str(&contents)?;
        Ok(file_uploads)
      }
      _ => Err(QueryError::Precondition("Not a files list")),
    };
  }
}

pub(crate) struct InsertQueryBuilder;

impl InsertQueryBuilder {
  pub(crate) async fn run<T: Send + 'static>(
    state: &AppState,
    params: Params,
    conflict_resolution: Option<ConflictResolutionStrategy>,
    return_column_name: Option<&str>,
    extractor: impl Fn(&rusqlite::Row) -> Result<T, trailbase_sqlite::Error> + Send + 'static,
  ) -> Result<T, QueryError> {
    let (query, named_params, mut files) =
      Self::build_insert_query(params, conflict_resolution, return_column_name)?;

    // We're storing any files to the object store first to make sure the DB entry is valid right
    // after commit and not racily pointing to soon-to-be-written files.
    if !files.is_empty() {
      let objectstore = state.objectstore();
      for (metadata, content) in &mut files {
        write_file(objectstore, metadata, content).await?;
      }
    }

    let result = state
      .conn()
      .call(move |conn| {
        let mut stmt = conn.prepare(&query)?;
        named_params.bind(&mut stmt)?;
        let mut result = stmt.raw_query();

        return match result.next()? {
          Some(row) => Ok(extractor(row)?),
          _ => Err(rusqlite::Error::QueryReturnedNoRows.into()),
        };
      })
      .await;

    if result.is_err() && !files.is_empty() {
      let objectstore = state.objectstore();

      for (metadata, _files) in &files {
        let path = object_store::path::Path::from(metadata.path());
        if let Err(err) = objectstore.delete(&path).await {
          warn!("Failed to cleanup file after failed insertion (leak): {err}");
        }
      }
    }

    return Ok(result?);
  }

  pub(crate) async fn run_bulk<T: Send + 'static>(
    state: &AppState,
    params_list: Vec<Params>,
    conflict_resolution: Option<ConflictResolutionStrategy>,
    return_column_name: Option<&str>,
    extractor: impl Fn(&rusqlite::Row) -> Result<T, trailbase_sqlite::Error> + Send + 'static,
  ) -> Result<Vec<T>, QueryError> {
    let mut all_files: FileMetadataContents = vec![];
    let mut query_and_params: Vec<(String, NamedParams)> = vec![];

    for params in params_list {
      let (query, named_params, mut files) =
        Self::build_insert_query(params, conflict_resolution, return_column_name)?;

      all_files.append(&mut files);
      query_and_params.push((query, named_params));
    }

    // We're storing any files to the object store first to make sure the DB entry is valid right
    // after commit and not racily pointing to soon-to-be-written files.
    if !all_files.is_empty() {
      let objectstore = state.objectstore();
      for (metadata, content) in &mut all_files {
        write_file(objectstore, metadata, content).await?;
      }
    }

    let result = state
      .conn()
      .call(move |conn| {
        let mut rows = Vec::<T>::with_capacity(query_and_params.len());

        let tx = conn.transaction()?;

        for (query, named_params) in query_and_params {
          let mut stmt = tx.prepare(&query)?;
          named_params.bind(&mut stmt)?;
          let mut result = stmt.raw_query();

          match result.next()? {
            Some(row) => rows.push(extractor(row)?),
            _ => {
              return Err(rusqlite::Error::QueryReturnedNoRows.into());
            }
          };
        }

        tx.commit()?;

        return Ok(rows);
      })
      .await;

    if result.is_err() && !all_files.is_empty() {
      let objectstore = state.objectstore();

      for (metadata, _files) in &all_files {
        let path = object_store::path::Path::from(metadata.path());
        if let Err(err) = objectstore.delete(&path).await {
          warn!("Failed to cleanup file after failed insertion (leak): {err}");
        }
      }
    }

    return Ok(result?);
  }

  fn build_insert_query(
    params: Params,
    conflict_resolution: Option<ConflictResolutionStrategy>,
    return_column_name: Option<&str>,
  ) -> Result<(String, NamedParams, FileMetadataContents), QueryError> {
    let table_name = params.table_name();

    let conflict_clause = Self::conflict_resolution_clause(
      conflict_resolution.unwrap_or(ConflictResolutionStrategy::Undefined),
    );

    let return_fragment: Cow<'_, str> = match return_column_name {
      Some(return_column_name) => {
        if return_column_name == "*" {
          Cow::Borrowed(r#"RETURNING *"#)
        } else {
          format!(r#"RETURNING "{return_column_name}""#).into()
        }
      }
      None => Cow::Borrowed(""),
    };

    let column_names = params.column_names();
    let query = if !column_names.is_empty() {
      format!(
        r#"INSERT {conflict_clause} INTO "{table_name}" ({col_names}) VALUES ({placeholders}) {return_fragment}"#,
        col_names = crate::schema::quote(column_names),
        placeholders = params.placeholders(),
      )
    } else {
      // The insert empty record case, i.e. "{}".
      format!(r#"INSERT {conflict_clause} INTO "{table_name}" DEFAULT VALUES {return_fragment}"#)
    };

    return Ok((query, params.named_params, params.files));
  }

  #[inline]
  fn conflict_resolution_clause(config: ConflictResolutionStrategy) -> &'static str {
    type C = ConflictResolutionStrategy;
    return match config {
      C::Undefined => "",
      C::Abort => "OR ABORT",
      C::Rollback => "OR ROLLBACK",
      C::Fail => "OR FAIL",
      C::Ignore => "OR IGNORE",
      C::Replace => "OR REPLACE",
    };
  }
}

pub(crate) struct UpdateQueryBuilder;

impl UpdateQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    metadata: &TableMetadata,
    mut params: Params,
    pk_column: &str,
    pk_value: Value,
  ) -> Result<(), QueryError> {
    let table_name = metadata.name();
    assert_eq!(params.table_name(), table_name);
    if params.column_names().is_empty() {
      return Ok(());
    }

    params.push_param(pk_column.to_string(), pk_value.clone());

    // We're storing to object store before writing the entry to the DB.
    let mut files = std::mem::take(&mut params.files);
    if !files.is_empty() {
      let objectstore = state.objectstore();
      for (metadata, content) in &mut files {
        write_file(objectstore, metadata, content).await?;
      }
    }

    async fn row_update(
      conn: &trailbase_sqlite::Connection,
      table_name: &str,
      params: Params,
      pk_column: &str,
      pk_value: Value,
    ) -> Result<Option<trailbase_sqlite::Row>, QueryError> {
      let setters: String = {
        let col_names = params.column_names();
        assert_eq!(col_names.len(), params.named_params.len());

        std::iter::zip(col_names, &params.named_params)
          .map(|(col_name, (placeholder, _value))| format!(r#""{col_name}" = {placeholder}"#))
          .join(", ")
      };

      let pk_column = pk_column.to_string();
      let table_name = table_name.to_string();
      let files_row = conn
        .call(move |conn| {
          let tx = conn.transaction()?;

          // First, fetch updated file column contents so we can delete the files after updating the
          // column.
          let file_col_names = params.file_column_names();
          let files_row = if file_col_names.is_empty() {
            None
          } else {
            let file_columns = file_col_names.join(", ");

            let mut stmt = tx.prepare(&format!(
              r#"SELECT {file_columns} FROM "{table_name}" WHERE "{pk_column}" = :{pk_column}"#
            ))?;

            use trailbase_sqlite::Params;
            [(":pk_column", pk_value)].bind(&mut stmt)?;

            let mut rows = stmt.raw_query();
            if let Some(row) = rows.next()? {
              Some(trailbase_sqlite::Row::from_row(row, None)?)
            } else {
              None
            }
          };

          // Update the column.
          {
            let mut stmt = tx.prepare(&format!(
              r#"UPDATE "{table_name}" SET {setters} WHERE "{pk_column}" = :{pk_column}"#
            ))?;
            use trailbase_sqlite::Params;
            params.named_params.bind(&mut stmt)?;

            stmt.raw_execute()?;
          }

          tx.commit()?;

          return Ok(files_row);
        })
        .await?;

      return Ok(files_row);
    }

    let files_row = match row_update(state.conn(), table_name, params, pk_column, pk_value).await {
      Ok(files_row) => files_row,
      Err(err) => {
        if !files.is_empty() {
          let store = state.objectstore();
          for (metadata, _content) in &files {
            let path = object_store::path::Path::from(metadata.path());
            if let Err(err) = store.delete(&path).await {
              warn!("Failed to cleanup file after failed insertion (leak): {err}");
            }
          }
        }

        return Err(err);
      }
    };

    // Finally, if everything else went well delete files from columns that were updated and are no
    // longer referenced.
    if let Some(files_row) = files_row {
      delete_files_in_row(state, metadata, files_row).await?;
    }

    return Ok(());
  }
}

pub(crate) struct DeleteQueryBuilder;

impl DeleteQueryBuilder {
  pub(crate) async fn run(
    state: &AppState,
    metadata: &TableMetadata,
    pk_column: &str,
    pk_value: Value,
  ) -> Result<(), QueryError> {
    let table_name = metadata.name();

    let row = state
      .conn()
      .query_row(
        &format!(r#"DELETE FROM "{table_name}" WHERE "{pk_column}" = $1 RETURNING *"#),
        [pk_value],
      )
      .await?
      .ok_or_else(|| QueryError::Sql(rusqlite::Error::QueryReturnedNoRows.into()))?;

    // Finally, delete files.
    delete_files_in_row(state, metadata, row).await?;

    return Ok(());
  }
}

async fn write_file(
  store: &dyn ObjectStore,
  metadata: &FileUpload,
  data: &mut Vec<u8>,
) -> Result<(), object_store::Error> {
  let path = object_store::path::Path::from(metadata.path());

  let mut writer = store.put_multipart(&path).await?;
  writer.put_part(std::mem::take(data).into()).await?;
  writer.complete().await?;

  return Ok(());
}
