use crate::error::Error;
use crate::params::Params;
use crate::rows::{Row, Rows};
use crate::sqlite::sync::SyncConnectionTrait;

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

impl<'a> SyncConnectionTrait for Transaction<'a> {
  // Queries the first row and returns it if present, otherwise `None`.
  #[inline]
  fn query_row(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    return SyncConnectionTrait::query_row(&*self.tx, sql, params);
  }

  #[inline]
  fn query_rows(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    return SyncConnectionTrait::query_rows(&*self.tx, sql, params);
  }

  #[inline]
  fn execute(&self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    return SyncConnectionTrait::execute(&*self.tx, sql, params);
  }

  #[inline]
  fn execute_batch(&self, sql: impl AsRef<str>) -> Result<(), Error> {
    return SyncConnectionTrait::execute_batch(&*self.tx, sql);
  }
}
