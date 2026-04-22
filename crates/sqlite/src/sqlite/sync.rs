use std::sync::Arc;

use crate::error::Error;
use crate::params::Params;
use crate::rows::{Row, Rows};
use crate::sqlite::util::{columns, from_row, from_rows};

pub trait SyncConnectionTrait {
  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error>;

  // Queries all rows. Materialization is eager, thus be careful with large results.
  // We may want to introduce a lazy version in the future.
  fn query_rows(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error>;

  // Executes the query and returns number of affected rows.
  fn execute(&self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error>;

  // Executes a batch of statements.
  fn execute_batch(&self, sql: impl AsRef<str>) -> Result<(), Error>;
}

pub struct SyncConnection<'a> {
  pub(crate) conn: &'a mut rusqlite::Connection,
}

impl<'a> SyncConnectionTrait for SyncConnection<'a> {
  #[inline]
  fn query_row(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    return SyncConnectionTrait::query_row(self.conn, sql, params);
  }

  #[inline]
  fn query_rows(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    return SyncConnectionTrait::query_rows(self.conn, sql, params);
  }

  #[inline]
  fn execute(&self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    return SyncConnectionTrait::execute(self.conn, sql, params);
  }

  #[inline]
  fn execute_batch(&self, sql: impl AsRef<str>) -> Result<(), Error> {
    return SyncConnectionTrait::execute_batch(self.conn, sql);
  }
}

impl SyncConnectionTrait for rusqlite::Connection {
  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    let mut stmt = self.prepare_cached(sql.as_ref())?;
    params.bind(&mut stmt)?;

    if let Some(row) = stmt.raw_query().next()? {
      return Ok(Some(from_row(row, Arc::new(columns(row.as_ref())))?));
    }
    return Ok(None);
  }

  fn query_rows(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    let mut stmt = self.prepare_cached(sql.as_ref())?;
    params.bind(&mut stmt)?;
    return from_rows(stmt.raw_query());
  }

  fn execute(&self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    let mut stmt = self.prepare_cached(sql.as_ref())?;
    params.bind(&mut stmt)?;
    return Ok(stmt.raw_execute()?);
  }

  fn execute_batch(&self, sql: impl AsRef<str>) -> Result<(), Error> {
    use rusqlite::fallible_iterator::FallibleIterator;

    let mut batch = rusqlite::Batch::new(self, sql.as_ref());
    while let Some(mut stmt) = batch.next()? {
      // NOTE: We must use `raw_query` instead of `raw_execute`, otherwise queries
      // returning rows (e.g. SELECT) will return an error. Rusqlite's batch_execute
      // behaves consistently.
      let _row = stmt.raw_query().next()?;
    }
    return Ok(());
  }
}
