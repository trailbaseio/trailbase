use crate::error::Error;
use crate::params::Params;
use crate::rows::{Row, Rows};
use crate::sqlite::sync::{execute, execute_batch, query_row, query_rows};
use crate::traits::{SyncConnection, SyncTransaction};

impl<'a> SyncConnection for rusqlite::Transaction<'a> {
  // Queries the first row and returns it if present, otherwise `None`.
  #[inline]
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    return query_row(self, sql, params);
  }

  #[inline]
  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    return query_rows(self, sql, params);
  }

  #[inline]
  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    return execute(self, sql, params);
  }

  #[inline]
  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    return execute_batch(self, sql);
  }
}

impl<'a> SyncTransaction for rusqlite::Transaction<'a> {
  fn commit(self) -> Result<(), Error> {
    self.commit()?;
    return Ok(());
  }

  fn rollback(self) -> Result<(), Error> {
    self.rollback()?;
    return Ok(());
  }

  fn expand_sql(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<String>, Error> {
    let mut stmt = self.prepare(sql.as_ref())?;
    params.bind(&mut stmt)?;
    return Ok(stmt.expanded_sql());
  }
}

pub struct Transaction<'a> {
  pub(crate) tx: rusqlite::Transaction<'a>,
}

impl<'a> SyncConnection for Transaction<'a> {
  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    return query_row(&self.tx, sql, params);
  }

  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    return query_rows(&self.tx, sql, params);
  }

  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    return execute(&self.tx, sql, params);
  }

  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    return execute_batch(&self.tx, sql);
  }
}

impl<'a> SyncTransaction for Transaction<'a> {
  fn commit(self) -> Result<(), Error> {
    self.tx.commit()?;
    return Ok(());
  }

  fn rollback(self) -> Result<(), Error> {
    self.tx.rollback()?;
    return Ok(());
  }

  fn expand_sql(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<String>, Error> {
    let mut stmt = self.tx.prepare(sql.as_ref())?;
    params.bind(&mut stmt)?;
    return Ok(stmt.expanded_sql());
  }
}
