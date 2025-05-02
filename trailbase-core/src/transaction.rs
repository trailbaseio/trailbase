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

pub struct TransactionLog {
  log: Vec<String>,
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
        .filter_map(|stmt| match stmt.as_str() {
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
}

/// A recorder for table migrations, i.e.: create, alter, drop, as opposed to data migrations.
pub struct TransactionRecorder<'a> {
  tx: rusqlite::Transaction<'a>,

  log: Vec<String>,
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
    let mut stmt = self.tx.prepare(sql)?;
    let mut rows = stmt.query(params)?;
    rows.next()?;
    self.log.push(sql.to_string());

    return Ok(());
  }

  pub fn execute(
    &mut self,
    sql: &str,
    params: impl rusqlite::Params,
  ) -> Result<usize, rusqlite::Error> {
    let rows_affected = self.tx.execute(sql, params)?;
    self.log.push(sql.to_string());
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

#[cfg(not(test))]
async fn write_migration_file(path: PathBuf, sql: &str) -> std::io::Result<()> {
  use tokio::io::AsyncWriteExt;

  let mut migration_file = tokio::fs::File::create_new(path).await?;
  migration_file.write_all(sql.as_bytes()).await?;
  return Ok(());
}

#[cfg(test)]
async fn write_migration_file(_path: PathBuf, _sql: &str) -> std::io::Result<()> {
  return Ok(());
}
