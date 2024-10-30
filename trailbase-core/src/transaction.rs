use libsql::{Connection, Rows, Transaction};
use log::*;
use refinery_libsql::LibsqlConnection;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::migrations;

#[derive(Debug, Error)]
pub enum TransactionError {
  #[error("Libsql error: {0}")]
  Libsql(#[from] libsql::Error),
  #[error("IO error: {0}")]
  IO(#[from] std::io::Error),
  #[error("Migration error: {0}")]
  Migration(#[from] refinery::Error),
  #[error("File error: {0}")]
  File(String),
}

/// A recorder for table migrations, i.e.: create, alter, drop, as opposed to data migrations.
pub struct TransactionRecorder {
  conn: Connection,
  tx: Transaction,
  log: Vec<String>,

  migration_path: PathBuf,
  migration_suffix: String,
}

#[allow(unused)]
impl TransactionRecorder {
  pub async fn new(
    conn: Connection,
    migration_path: PathBuf,
    migration_suffix: String,
  ) -> Result<Self, TransactionError> {
    let tx = conn.transaction().await?;

    return Ok(TransactionRecorder {
      conn,
      tx,
      log: vec![],
      migration_path,
      migration_suffix,
    });
  }

  // Note that we cannot take any sql params for recording purposes.
  pub async fn query(&mut self, sql: &str) -> Result<Rows, TransactionError> {
    let rows = self.tx.query(sql, ()).await?;
    self.log.push(sql.to_string());
    return Ok(rows);
  }

  pub async fn execute(&mut self, sql: &str) -> Result<u64, TransactionError> {
    let rows_affected = self.tx.execute(sql, ()).await?;
    self.log.push(sql.to_string());
    return Ok(rows_affected);
  }

  /// Consume this transaction and commit.
  pub async fn commit_and_create_migration(
    self,
  ) -> Result<Option<refinery::Report>, TransactionError> {
    if self.log.is_empty() {
      return Ok(None);
    }

    // We have to commit alter table transactions through refinery to keep the migration table in
    // sync.
    // NOTE: Slightly hacky that we build up the transaction first to then cancel it. However, this
    // gives us early checking. We could as well just not do it.
    self.tx.rollback().await?;

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
      sqlformat::FormatOptions {
        indent: sqlformat::Indent::Spaces(4),
        uppercase: true,
        lines_between_queries: 2,
      },
    );

    let migrations = vec![refinery::Migration::unapplied(&stem, &sql)?];

    let mut conn = LibsqlConnection::from_connection(self.conn);
    let mut runner = migrations::new_migration_runner(&migrations).set_abort_missing(false);

    let report = runner.run_async(&mut conn).await.map_err(|err| {
      error!("Migration aborted with: {err} for {sql}");
      err
    })?;

    write_migration_file(path, &sql).await?;

    return Ok(Some(report));
  }

  /// Consume this transaction and rollback.
  pub async fn rollback(self) -> Result<(), TransactionError> {
    return Ok(self.tx.rollback().await?);
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
