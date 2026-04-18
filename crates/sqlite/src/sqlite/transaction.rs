use std::sync::Arc;

use crate::error::Error;
use crate::params::Params;
use crate::rows::Row;
use crate::sqlite::util::{columns, from_row};

pub struct Transaction<'a> {
  tx: rusqlite::Transaction<'a>,
}

impl<'a> Transaction<'a> {
  pub fn new(tx: rusqlite::Transaction<'a>) -> Self {
    return Self { tx };
  }

  pub fn commit(self) -> Result<(), Error> {
    self.tx.commit()?;
    return Ok(());
  }

  pub fn rollback(self) -> Result<(), Error> {
    self.tx.rollback()?;
    return Ok(());
  }

  pub fn execute(&self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    let mut stmt = self.tx.prepare(sql.as_ref())?;
    params.bind(&mut stmt)?;
    return Ok(stmt.raw_execute()?);
  }

  pub fn execute_batch(&self, sql: impl AsRef<str>) -> Result<(), Error> {
    use rusqlite::fallible_iterator::FallibleIterator;

    let mut batch = rusqlite::Batch::new(&self.tx, sql.as_ref());
    while let Some(mut stmt) = batch.next()? {
      // NOTE: We must use `raw_query` instead of `raw_execute`, otherwise queries
      // returning rows (e.g. SELECT) will return an error. Rusqlite's batch_execute
      // behaves consistently.
      let _row = stmt.raw_query().next()?;
    }
    return Ok(());
  }

  // pub fn query_row_get<T>(
  //   &self,
  //   sql: impl AsRef<str>,
  //   params: impl Params,
  //   index: usize,
  // ) -> Result<Option<T>, Error>
  // where
  //   T: FromSql + Send + 'static,
  // {
  //   let mut stmt = self.tx.prepare(sql.as_ref())?;
  //   params.bind(&mut stmt)?;
  //
  //   if let Some(row) = stmt.raw_query().next()? {
  //     return get_value(row, index);
  //   }
  //   return Ok(None);
  // }

  // Queries the first row and returns it if present, otherwise `None`.
  pub fn query_row(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    let mut stmt = self.tx.prepare(sql.as_ref())?;
    params.bind(&mut stmt)?;

    if let Some(row) = stmt.raw_query().next()? {
      return Ok(Some(from_row(row, Arc::new(columns(row.as_ref())))?));
    }
    return Ok(None);
  }

  // pub fn query_rows(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
  //   let mut stmt = self.tx.prepare(sql.as_ref())?;
  //   params.bind(&mut stmt)?;
  //
  //   return from_rows(stmt.raw_query());
  // }

  pub fn expand_sql(
    &self,
    sql: impl AsRef<str>,
    params: impl Params,
  ) -> Result<Option<String>, Error> {
    let mut stmt = self.tx.prepare(sql.as_ref())?;
    params.bind(&mut stmt)?;
    return Ok(stmt.expanded_sql());
  }
}
