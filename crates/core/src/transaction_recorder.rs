use log::*;
use std::{
  io::Write,
  path::{Path, PathBuf},
};
use thiserror::Error;

use crate::migrations;

#[derive(Debug, Error)]
pub enum TransactionError {
  #[error("SQLite error: {0}")]
  Sqlite(#[from] trailbase_sqlite::Error),
  #[error("IO error: {0}")]
  IO(#[from] std::io::Error),
  #[error("Migration error: {0}")]
  Migration(#[from] trailbase_refinery::Error),
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
  pub(crate) fn build_sql(&self) -> String {
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

    return sqlformat::format(
      &sql_string,
      &sqlformat::QueryParams::None,
      &sqlformat::FormatOptions {
        ignore_case_convert: None,
        indent: sqlformat::Indent::Spaces(4),
        uppercase: Some(true),
        lines_between_queries: 2,
        ..Default::default()
      },
    );
  }

  /// Commit previously recorded transaction log on provided connection.
  pub(crate) async fn apply_as_migration(
    &self,
    conn: &trailbase_sqlite::Connection,
    migration_path: impl AsRef<Path>,
    filename_suffix: &str,
  ) -> Result<trailbase_refinery::Report, TransactionError> {
    let filename = migrations::new_unique_migration_filename(filename_suffix);
    let stem = Path::new(&filename)
      .file_stem()
      .ok_or_else(|| TransactionError::File(format!("Failed to get stem from: {filename}")))?
      .to_string_lossy()
      .to_string();
    let path = migration_path.as_ref().join(filename);

    let sql = self.build_sql();
    let migrations = vec![trailbase_refinery::Migration::unapplied(&stem, &sql)?];
    let runner = migrations::new_migration_runner(&migrations).set_abort_missing(false);

    let report = conn
      .call_writer(move |conn| {
        return runner
          .run(conn)
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await
      .map_err(|err| {
        error!("Migration aborted with: {err} for {sql}");
        err
      })?;

    write_migration_file(path, &sql)?;

    return Ok(report);
  }

  #[cfg(test)]
  pub(crate) async fn commit(
    self,
    conn: &trailbase_sqlite::Connection,
  ) -> Result<(), trailbase_sqlite::Error> {
    conn
      .transaction(|tx| -> Result<(), trailbase_sqlite::Error> {
        for (query_type, stmt) in self.log {
          match query_type {
            QueryType::Query => {
              // TODO: Maybe we should have a query option returning nothing :shrug:.
              tx.query_row_get::<trailbase_sqlite::Value>(stmt, (), 0)?;
            }
            QueryType::Execute => {
              tx.execute(stmt, ())?;
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
  tx: trailbase_sqlite::Transaction<'a>,
  log: Vec<(QueryType, String)>,
}

impl<'a> TransactionRecorder<'a> {
  pub fn new(tx: trailbase_sqlite::Transaction<'a>) -> Self {
    return Self { tx, log: vec![] };
  }

  // Note that we cannot take any sql params for recording purposes.
  #[allow(unused)]
  pub fn query(
    &mut self,
    sql: &str,
    params: impl trailbase_sqlite::Params + Clone,
  ) -> Result<(), trailbase_sqlite::Error> {
    // First get the SQL. The other way round there's some validation issues.
    let Some(expanded_sql) = self.tx.expand_sql(sql, params.clone())? else {
      return Err(trailbase_sqlite::Error::Other(
        format!("failed to get expanded query for: {sql}").into(),
      ));
    };

    self
      .tx
      .query_row_get::<trailbase_sqlite::Value>(sql, params, 0)?;

    self.log.push((QueryType::Query, expanded_sql));

    return Ok(());
  }

  pub fn execute(
    &mut self,
    sql: &str,
    params: impl trailbase_sqlite::Params + Clone,
  ) -> Result<usize, trailbase_sqlite::Error> {
    // First get the SQL. The other way round there's some validation issues.
    let Some(expanded_sql) = self.tx.expand_sql(sql, params.clone())? else {
      return Err(trailbase_sqlite::Error::Other(
        format!("failed to get expanded execute query for: {sql}").into(),
      ));
    };

    let rows_affected = self.tx.execute(sql, params)?;

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

fn write_migration_file(path: PathBuf, sql: &str) -> std::io::Result<()> {
  if cfg!(test) {
    return Ok(());
  }

  if let Some(parent) = path.parent() {
    // Returns error if path already exists.
    if !parent.exists() {
      std::fs::create_dir(parent)?;
    }
  }
  let mut migration_file = std::fs::File::create_new(path)?;
  migration_file.write_all(sql.as_bytes())?;
  return Ok(());
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_transaction_log() {
    let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
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
      .await
      .unwrap();

    // Just double checking that rusqlite's query and execute ignore everything but the first
    // statement.
    let result = conn
      .write_query_row_get::<String>(
        r#"
          SELECT name FROM 'table' WHERE id = 0;
          SELECT name FROM 'table' WHERE id = 1;
          DROP TABLE 'table';
        "#,
        (),
        0,
      )
      .await;
    assert!(matches!(
      result,
      Err(trailbase_sqlite::Error::Rusqlite(
        rusqlite::Error::MultipleStatement
      ))
    ));

    let log = conn
      .transaction(|tx| -> Result<_, trailbase_sqlite::Error> {
        let mut recorder = TransactionRecorder::new(tx);

        recorder
          .execute("DELETE FROM 'table' WHERE age < ?1", (20,))
          .unwrap();

        return Ok(recorder.rollback().unwrap().unwrap());
      })
      .await
      .unwrap();

    assert_eq!(log.log.len(), 1);
    assert_eq!(log.log[0].0, QueryType::Execute);
    assert_eq!(log.log[0].1, "DELETE FROM 'table' WHERE age < 20");

    let count: i64 = conn
      .read_query_row_get("SELECT COUNT(*) FROM 'table'", (), 0)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(count, 2);

    log.commit(&conn).await.unwrap();

    let count: i64 = conn
      .read_query_row_get("SELECT COUNT(*) FROM 'table'", (), 0)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(count, 1);
  }
}
