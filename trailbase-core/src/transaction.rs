use log::*;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::migrations;

#[derive(Debug, Error)]
pub enum TransactionError {
  #[error("Rusqlite error: {0}")]
  Rusqlite(#[from] rusqlite::Error),
  #[error("Tokio Rusqlite error: {0}")]
  TokioRusqlite(#[from] trailbase_sqlite::Error),
  #[error("IO error: {0}")]
  IO(#[from] std::io::Error),
  #[error("Migration error: {0}")]
  Migration(#[from] trailbase_refinery_core::Error),
  #[error("File error: {0}")]
  File(String),
}

#[derive(Clone, Debug, PartialEq)]
enum QueryType {
  Query,
  Execute,
}

pub struct TransactionLog {
  log: Vec<(QueryType, String)>,
}

impl TransactionLog {
  /// Commit previously recorded transaction log on provided connection.
  pub(crate) async fn apply_as_migration(
    &self,
    conn: &trailbase_sqlite::Connection,
    migration_path: impl AsRef<Path>,
    filename_suffix: &str,
  ) -> Result<trailbase_refinery_core::Report, TransactionError> {
    let filename = migrations::new_unique_migration_filename(filename_suffix);
    let stem = Path::new(&filename)
      .file_stem()
      .ok_or_else(|| TransactionError::File(format!("Failed to get stem from: {filename}")))?
      .to_string_lossy()
      .to_string();
    let path = migration_path.as_ref().join(filename);

    let sql = {
      let sql_string: String = self
        .log
        .iter()
        .filter_map(|(_, stmt)| match stmt.as_str() {
          "" => None,
          x if x.ends_with(";") => Some(stmt.clone()),
          x => Some(format!("{x};")),
        })
        .collect::<Vec<String>>()
        .join("\n");

      sqlformat::format(
        &sql_string,
        &sqlformat::QueryParams::None,
        &sqlformat::FormatOptions {
          ignore_case_convert: None,
          indent: sqlformat::Indent::Spaces(4),
          uppercase: Some(true),
          lines_between_queries: 2,
        },
      )
    };

    let migrations = vec![trailbase_refinery_core::Migration::unapplied(&stem, &sql)?];
    let runner = migrations::new_migration_runner(&migrations).set_abort_missing(false);

    let report = conn
      .call(move |conn| {
        let report = runner
          .run(conn)
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

        return Ok(report);
      })
      .await
      .map_err(|err| {
        error!("Migration aborted with: {err} for {}", sql);
        err
      })?;

    write_migration_file(path, &sql).await?;

    return Ok(report);
  }

  #[allow(unused)]
  pub(crate) async fn commit(
    self,
    conn: &trailbase_sqlite::Connection,
  ) -> Result<(), trailbase_sqlite::Error> {
    conn
      .call(|conn: &mut rusqlite::Connection| {
        let tx = conn.transaction()?;
        for (query_type, stmt) in self.log {
          match query_type {
            QueryType::Query => {
              tx.query_row(&stmt, (), |_row| Ok(()))?;
            }
            QueryType::Execute => {
              tx.execute(&stmt, ())?;
            }
          }
        }
        tx.commit()?;

        return Ok(());
      })
      .await?;

    return Ok(());
  }
}

/// A recorder for table migrations, i.e.: create, alter, drop, as opposed to data migrations.
pub struct TransactionRecorder<'a> {
  tx: rusqlite::Transaction<'a>,

  log: Vec<(QueryType, String)>,
}

impl<'a> TransactionRecorder<'a> {
  pub fn new(conn: &'a mut rusqlite::Connection) -> Result<Self, rusqlite::Error> {
    let recorder = TransactionRecorder {
      tx: conn.transaction()?,
      log: vec![],
    };

    return Ok(recorder);
  }

  // Note that we cannot take any sql params for recording purposes.
  #[allow(unused)]
  pub fn query(&mut self, sql: &str, params: impl rusqlite::Params) -> Result<(), rusqlite::Error> {
    let mut stmt = self.tx.prepare_cached(sql)?;
    params.__bind_in(&mut stmt)?;
    let Some(expanded_sql) = stmt.expanded_sql() else {
      return Err(rusqlite::Error::ToSqlConversionFailure(
        "failed to get expanded query".into(),
      ));
    };

    let mut rows = stmt.raw_query();
    rows.next()?;

    self.log.push((QueryType::Query, expanded_sql));

    return Ok(());
  }

  pub fn execute(
    &mut self,
    sql: &str,
    params: impl rusqlite::Params,
  ) -> Result<usize, rusqlite::Error> {
    // let rows_affected = self.tx.execute(sql, params)?;
    let mut stmt = self.tx.prepare_cached(sql)?;
    params.__bind_in(&mut stmt)?;
    let Some(expanded_sql) = stmt.expanded_sql() else {
      return Err(rusqlite::Error::ToSqlConversionFailure(
        "failed to get expanded query".into(),
      ));
    };

    let rows_affected = stmt.raw_execute()?;

    self.log.push((QueryType::Execute, expanded_sql));
    return Ok(rows_affected);
  }

  /// Consume this transaction and rollback.
  #[allow(unused)]
  pub fn rollback(mut self) -> Result<Option<TransactionLog>, TransactionError> {
    self.tx.rollback()?;

    if self.log.is_empty() {
      return Ok(None);
    }

    return Ok(Some(TransactionLog { log: self.log }));
  }
}

async fn write_migration_file(path: PathBuf, sql: &str) -> std::io::Result<()> {
  if cfg!(test) {
    return Ok(());
  }

  use tokio::io::AsyncWriteExt;

  let mut migration_file = tokio::fs::File::create_new(path).await?;
  migration_file.write_all(sql.as_bytes()).await?;
  return Ok(());
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_transaction_log() {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    conn
      .execute_batch(
        r#"
          CREATE TABLE 'table' (
            id    INTEGER PRIMARY KEY NOT NULL,
            name  TEXT NOT NULL,
            age   INTEGER
          ) STRICT;

          INSERT INTO 'table' (id, name, age) VALUES (0, 'Alice', 21), (1, 'Bob', 18);
        "#,
      )
      .unwrap();

    // Just double checking that rusqlite's query and execute ignore everything but the first
    // statement.
    let result = conn.query_row(
      r#"
          SELECT name FROM 'table' WHERE id = 0;
          SELECT name FROM 'table' WHERE id = 1;
          DROP TABLE 'table';
        "#,
      (),
      |row| row.get::<_, String>(0),
    );
    assert!(matches!(result, Err(rusqlite::Error::MultipleStatement)));

    let mut recorder = TransactionRecorder::new(&mut conn).unwrap();

    recorder
      .execute("DELETE FROM 'table' WHERE age < ?1", rusqlite::params!(20))
      .unwrap();
    let log = recorder.rollback().unwrap().unwrap();

    assert_eq!(log.log.len(), 1);
    assert_eq!(log.log[0].0, QueryType::Execute);
    assert_eq!(log.log[0].1, "DELETE FROM 'table' WHERE age < 20");

    let conn = trailbase_sqlite::Connection::from_connection_test_only(conn);
    let count: i64 = conn
      .query_row_f("SELECT COUNT(*) FROM 'table'", (), |row| row.get(0))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(count, 2);

    log.commit(&conn).await.unwrap();

    let count: i64 = conn
      .query_row_f("SELECT COUNT(*) FROM 'table'", (), |row| row.get(0))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(count, 1);
  }
}
