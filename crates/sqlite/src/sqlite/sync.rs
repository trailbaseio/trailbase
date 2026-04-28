use std::sync::Arc;

use crate::error::Error;
use crate::params::Params;
use crate::rows::{Row, Rows};
use crate::sqlite::util::{columns, from_row, from_rows};
use crate::traits::SyncConnection as SyncConnectionTrait;

pub struct SyncConnection<'a> {
  pub(crate) conn: &'a mut rusqlite::Connection,
}

impl<'a> SyncConnectionTrait for SyncConnection<'a> {
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    return query_row(self.conn, sql, params);
  }

  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    return query_rows(self.conn, sql, params);
  }

  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    return execute(self.conn, sql, params);
  }

  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    return execute_batch(self.conn, sql);
  }
}

impl SyncConnectionTrait for rusqlite::Connection {
  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    return query_row(self, sql, params);
  }

  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    return query_rows(self, sql, params);
  }

  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    return execute(self, sql, params);
  }

  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    return execute_batch(self, sql);
  }
}

#[inline]
pub(super) fn query_row(
  conn: &rusqlite::Connection,
  sql: impl AsRef<str>,
  params: impl Params,
) -> Result<Option<Row>, Error> {
  let mut stmt = conn.prepare_cached(sql.as_ref())?;
  params.bind(&mut stmt)?;

  if let Some(row) = stmt.raw_query().next()? {
    return Ok(Some(from_row(row, Arc::new(columns(row.as_ref())))?));
  }
  return Ok(None);
}

#[inline]
pub(super) fn query_rows(
  conn: &rusqlite::Connection,
  sql: impl AsRef<str>,
  params: impl Params,
) -> Result<Rows, Error> {
  let mut stmt = conn.prepare_cached(sql.as_ref())?;
  params.bind(&mut stmt)?;
  return from_rows(stmt.raw_query());
}

#[inline]
pub(super) fn execute(
  conn: &rusqlite::Connection,
  sql: impl AsRef<str>,
  params: impl Params,
) -> Result<usize, Error> {
  let mut stmt = conn.prepare_cached(sql.as_ref())?;
  params.bind(&mut stmt)?;
  return Ok(stmt.raw_execute()?);
}

#[inline]
pub(super) fn execute_batch(
  conn: &rusqlite::Connection,
  sql: impl AsRef<str>,
) -> Result<(), Error> {
  use rusqlite::fallible_iterator::FallibleIterator;

  let mut batch = rusqlite::Batch::new(conn, sql.as_ref());
  while let Some(mut stmt) = batch.next()? {
    // NOTE: We must use `raw_query` instead of `raw_execute`, otherwise queries
    // returning rows (e.g. SELECT) will return an error. Rusqlite's batch_execute
    // behaves consistently.
    let _row = stmt.raw_query().next()?;
  }
  return Ok(());
}
