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

pub struct MigrationWriter {
  path: PathBuf,
  stem: String,
  sql: String,
}

impl MigrationWriter {
  pub(crate) async fn write(
    &self,
    conn: &trailbase_sqlite::Connection,
  ) -> Result<trailbase_refinery_core::Report, TransactionError> {
    let migrations = vec![trailbase_refinery_core::Migration::unapplied(
      &self.stem, &self.sql,
    )?];
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
        error!("Migration aborted with: {err} for {}", self.sql);
        err
      })?;

    write_migration_file(self.path.clone(), &self.sql).await?;

    return Ok(report);
  }
}

/// A recorder for table migrations, i.e.: create, alter, drop, as opposed to data migrations.
pub struct TransactionRecorder<'a> {
  tx: rusqlite::Transaction<'a>,

  log: Vec<String>,

  migration_path: PathBuf,
  migration_suffix: String,
}

#[allow(unused)]
impl<'a> TransactionRecorder<'a> {
  pub fn new(
    conn: &'a mut rusqlite::Connection,
    migration_path: PathBuf,
    migration_suffix: String,
  ) -> Result<Self, rusqlite::Error> {
    let recorder = TransactionRecorder {
      tx: conn.transaction()?,
      log: vec![],
      migration_path,
      migration_suffix,
    };

    return Ok(recorder);
  }

  // Note that we cannot take any sql params for recording purposes.
  pub fn query(&mut self, sql: &str) -> Result<(), rusqlite::Error> {
    let mut stmt = self.tx.prepare(sql)?;
    let mut rows = stmt.query([])?;
    rows.next()?;
    self.log.push(sql.to_string());

    return Ok(());
  }

  pub fn execute(&mut self, sql: &str) -> Result<usize, rusqlite::Error> {
    let rows_affected = self.tx.execute(sql, ())?;
    self.log.push(sql.to_string());
    return Ok(rows_affected);
  }

  /// Consume this transaction and commit.
  pub fn rollback_and_create_migration(
    mut self,
  ) -> Result<Option<MigrationWriter>, TransactionError> {
    if self.log.is_empty() {
      return Ok(None);
    }

    // We have to commit alter table transactions through refinery to keep the migration table in
    // sync.
    // NOTE: Slightly hacky that we build up the transaction first to then cancel it. However, this
    // gives us early checking. We could as well just not do it.
    self.tx.rollback()?;

    let filename = migrations::new_unique_migration_filename(&self.migration_suffix);
    let stem = Path::new(&filename)
      .file_stem()
      .ok_or_else(|| TransactionError::File(format!("Failed to get stem from: {filename}")))?
      .to_string_lossy()
      .to_string();
    let path = self.migration_path.join(filename);

    let mut sql: String = self
      .log
      .iter()
      .filter_map(|stmt| match stmt.as_str() {
        "" => None,
        x if x.ends_with(";") => Some(stmt.clone()),
        x => Some(format!("{x};")),
      })
      .collect::<Vec<String>>()
      .join("\n");

    sql = sqlformat::format(
      sql.as_str(),
      &sqlformat::QueryParams::None,
      &sqlformat::FormatOptions {
        ignore_case_convert: None,
        indent: sqlformat::Indent::Spaces(4),
        uppercase: Some(true),
        lines_between_queries: 2,
      },
    );

    return Ok(Some(MigrationWriter { path, stem, sql }));
  }

  /// Consume this transaction and rollback.
  pub fn rollback(mut self) -> Result<(), TransactionError> {
    self.tx.rollback()?;
    return Ok(());
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
