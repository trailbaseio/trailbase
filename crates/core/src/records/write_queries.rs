use askama::Template;
use object_store::ObjectStore;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;
use trailbase_schema::QualifiedNameEscaped;
use trailbase_sqlite::{Connection, NamedParams, Params as _, Value};

use crate::config::proto::ConflictResolutionStrategy;
use crate::records::error::RecordError;
use crate::records::files::{FileManager, delete_files_marked_for_deletion};
use crate::records::params::{FileMetadataContents, Params};

pub enum WriteQuery {
  Insert {
    query: String,
    named_params: NamedParams,
  },
  Update {
    query: String,
    named_params: NamedParams,
  },
  Delete {
    query: String,
    pk_value: Value,
  },
}

pub struct WriteQueryResult {
  pub rowid: i64,
  pub pk_value: Option<Value>,
}

impl WriteQuery {
  pub fn new_insert(
    table_name: &QualifiedNameEscaped,
    return_column_name: &str,
    conflict_resolution: Option<ConflictResolutionStrategy>,
    params: Params,
  ) -> Result<(Self, FileMetadataContents), RecordError> {
    let Params::Insert {
      named_params,
      files,
      column_names,
      column_indexes: _,
    } = params
    else {
      return Err(RecordError::Internal("not an insert".into()));
    };

    let conflict_clause = match conflict_resolution {
      Some(ConflictResolutionStrategy::Abort) => "OR ABORT",
      Some(ConflictResolutionStrategy::Rollback) => "OR ROLLBACK",
      Some(ConflictResolutionStrategy::Fail) => "OR FAIL",
      Some(ConflictResolutionStrategy::Ignore) => "OR IGNORE",
      Some(ConflictResolutionStrategy::Replace) => "OR REPLACE",
      _ => "",
    };

    let returning = &["_rowid_", return_column_name];

    let query = CreateRecordQueryTemplate {
      table_name,
      conflict_clause,
      column_names: &column_names,
      returning,
    }
    .render()
    .map_err(|err| RecordError::Internal(err.into()))?;

    return Ok((
      Self::Insert {
        query,
        named_params,
      },
      files,
    ));
  }

  pub fn new_update(
    table_name: &QualifiedNameEscaped,
    params: Params,
  ) -> Result<(Self, FileMetadataContents), RecordError> {
    let Params::Update {
      named_params,
      files,
      column_names,
      column_indexes: _,
      pk_column_name,
    } = params
    else {
      return Err(RecordError::Internal("not an update".into()));
    };

    let query = UpdateRecordQueryTemplate {
      table_name,
      column_names: &column_names,
      pk_column_name: &pk_column_name,
      returning: Some("_rowid_"),
    }
    .render()
    .map_err(|err| RecordError::Internal(err.into()))?;

    return Ok((
      Self::Update {
        query,
        named_params,
      },
      files,
    ));
  }

  pub fn new_delete(
    table_name: &QualifiedNameEscaped,
    pk_column_name: &str,
    pk_value: Value,
  ) -> Result<Self, RecordError> {
    return Ok(Self::Delete {
      query: format!(r#"DELETE FROM {table_name} WHERE "{pk_column_name}" = $1 RETURNING _rowid_"#),
      pk_value,
    });
  }

  pub fn apply(self, conn: &rusqlite::Connection) -> Result<WriteQueryResult, rusqlite::Error> {
    return match self {
      Self::Insert {
        query,
        named_params,
      } => {
        let mut stmt = conn.prepare_cached(query.as_ref())?;
        named_params.bind(&mut stmt)?;
        if let Some(row) = stmt.raw_query().next()? {
          Ok(WriteQueryResult {
            rowid: row.get(0)?,
            pk_value: Some(row.get(1)?),
          })
        } else {
          Err(rusqlite::Error::QueryReturnedNoRows)
        }
      }
      Self::Update {
        query,
        named_params,
      } => {
        let mut stmt = conn.prepare_cached(query.as_ref())?;
        named_params.bind(&mut stmt)?;
        if let Some(row) = stmt.raw_query().next()? {
          Ok(WriteQueryResult {
            rowid: row.get(0)?,
            pk_value: None,
          })
        } else {
          Err(rusqlite::Error::QueryReturnedNoRows)
        }
      }
      Self::Delete { query, pk_value } => Ok(WriteQueryResult {
        rowid: conn.query_row(&query, [pk_value], |row| row.get(0))?,
        pk_value: None,
      }),
    };
  }
}

pub(crate) async fn run_queries(
  conn: &Connection,
  objectstore: &Arc<dyn ObjectStore>,
  queries: Vec<(
    WriteQuery,
    Option<(QualifiedNameEscaped, FileMetadataContents)>,
  )>,
) -> Result<Vec<rusqlite::types::Value>, RecordError> {
  let (queries, all_files): (Vec<_>, Vec<_>) = queries.into_iter().unzip();

  let mut queries_with_files = HashMap::<QualifiedNameEscaped, Vec<usize>>::new();
  let all_files: FileMetadataContents = all_files
    .into_iter()
    .enumerate()
    .flat_map(|(i, files)| {
      if let Some(files) = files {
        match queries_with_files.entry(files.0) {
          Entry::Occupied(mut entry) => {
            entry.get_mut().push(i);
          }
          Entry::Vacant(entry) => {
            entry.insert(vec![i]);
          }
        }
        return files.1;
      }
      return vec![];
    })
    .collect();

  // We're storing any files to the object store first to make sure the DB entry is valid right
  // after commit and not racily pointing to soon-to-be-written files.
  let file_manager = if all_files.is_empty() {
    None
  } else {
    Some(FileManager::write(objectstore, all_files).await?)
  };

  let result: Vec<WriteQueryResult> = conn
    .call(move |conn| {
      let tx = conn.transaction()?;

      let rows: Vec<WriteQueryResult> = queries
        .into_iter()
        .map(|query| query.apply(&tx))
        .collect::<Result<Vec<_>, _>>()?;

      tx.commit()?;

      return Ok(rows);
    })
    .await?;

  if let Some(mut file_manager) = file_manager {
    // Successful transaction, do not cleanup written files. Then clean files marked for deletion.
    file_manager.release();

    for (table_name, indexes) in queries_with_files {
      let rowids: Vec<_> = indexes.into_iter().map(|i| result[i].rowid).collect();
      if let Err(err) =
        delete_files_marked_for_deletion(conn, objectstore, &table_name, &rowids).await
      {
        log::debug!("Failed deleting files: {err}");
      }
    }
  }

  return Ok(result.into_iter().filter_map(|r| r.pk_value).collect());
}

pub(crate) async fn run_insert_query(
  conn: &Connection,
  objectstore: &Arc<dyn ObjectStore>,
  table_name: &QualifiedNameEscaped,
  conflict_resolution: Option<ConflictResolutionStrategy>,
  return_column_name: &str,
  params: Params,
) -> Result<rusqlite::types::Value, RecordError> {
  let (query, files) =
    WriteQuery::new_insert(table_name, return_column_name, conflict_resolution, params)?;

  // We're storing any files to the object store first to make sure the DB entry is valid right
  // after commit and not racily pointing to soon-to-be-written files.
  let file_manager = if files.is_empty() {
    None
  } else {
    Some(FileManager::write(objectstore, files).await?)
  };

  let (rowid, return_value): (i64, rusqlite::types::Value) = conn
    .call(move |conn| {
      let result = query.apply(conn)?;
      return Ok((result.rowid, result.pk_value.expect("insert")));
    })
    .await?;

  // Successful write, do not cleanup written files.
  if let Some(mut file_manager) = file_manager {
    file_manager.release();

    if Some(ConflictResolutionStrategy::Replace) == conflict_resolution {
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
  params: Params,
) -> Result<(), RecordError> {
  let (query, files) = WriteQuery::new_update(table_name, params)?;

  // We're storing any files to the object store first to make sure the DB entry is valid right
  // after commit and not racily pointing to soon-to-be-written files.
  let file_manager = if files.is_empty() {
    None
  } else {
    Some(FileManager::write(objectstore, files).await?)
  };

  let rowid: i64 = conn
    .call(move |conn| {
      return Ok(query.apply(conn)?.rowid);
    })
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
  let query = WriteQuery::new_delete(table_name, pk_column, pk_value)?;

  let rowid: i64 = conn
    .call(move |conn| {
      return Ok(query.apply(conn)?.rowid);
    })
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
  column_names: &'a [String],
  pk_column_name: &'a str,
  returning: Option<&'a str>,
}

#[derive(Template)]
#[template(escape = "none", path = "create_record_query.sql")]
struct CreateRecordQueryTemplate<'a> {
  table_name: &'a QualifiedNameEscaped,
  conflict_clause: &'a str,
  column_names: &'a [String],
  returning: &'a [&'a str],
}

#[cfg(test)]
mod tests {
  use super::*;
  use trailbase_schema::parse::parse_into_statement;
  use trailbase_schema::sqlite::QualifiedName;

  fn sanitize_template(template: &str) {
    assert!(parse_into_statement(template).is_ok(), "{template}");
    assert!(!template.contains("\n\n"), "{template}");
    assert!(!template.contains("   "), "{template}");
  }

  #[test]
  fn test_create_record_template() {
    {
      let query = CreateRecordQueryTemplate {
        table_name: &QualifiedName::parse("table").unwrap().into(),
        conflict_clause: "OR ABORT",
        column_names: &["index".to_string(), "trigger".to_string()],
        returning: &["index"],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = CreateRecordQueryTemplate {
        table_name: &QualifiedName {
          name: "table".to_string(),
          database_schema: Some("db".to_string()),
        }
        .into(),
        conflict_clause: "",
        column_names: &[],
        returning: &["*"],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = CreateRecordQueryTemplate {
        table_name: &QualifiedName::parse("table").unwrap().into(),
        conflict_clause: "",
        column_names: &["index".to_string()],
        returning: &[],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }
  }
}
