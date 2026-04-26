use crate::error::Error;
use crate::params::Params;
use crate::pg::util::bind;
use crate::rows::{Row, Rows};
use crate::traits::SyncConnection as SyncConnectionTrait;
use crate::traits::SyncTransaction as SyncTransactionTrait;
use crate::value::Value;

impl SyncConnectionTrait for postgres::Client {
  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    // FIXME: Add missing impl.
    let params: Vec<Value> = bind(sql.as_ref(), params)?;
    // self.query_raw(sql.as_ref(), &params)?;
    // self.query(sql.as_ref(), &params);
    return Err(Error::NotSupported);
  }

  fn query_rows(&self, _sql: impl AsRef<str>, _params: impl Params) -> Result<Rows, Error> {
    // FIXME: Add missing impl.
    return Err(Error::NotSupported);
  }

  fn execute(&self, _sql: impl AsRef<str>, _params: impl Params) -> Result<usize, Error> {
    // FIXME: Add missing impl.
    return Err(Error::NotSupported);
  }

  fn execute_batch(&self, _sql: impl AsRef<str>) -> Result<(), Error> {
    // FIXME: Add missing impl.
    return Err(Error::NotSupported);
  }
}

impl<'a> SyncConnectionTrait for postgres::Transaction<'a> {
  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&self, _sql: impl AsRef<str>, _params: impl Params) -> Result<Option<Row>, Error> {
    // FIXME: Add missing impl.
    return Err(Error::NotSupported);
  }

  fn query_rows(&self, _sql: impl AsRef<str>, _params: impl Params) -> Result<Rows, Error> {
    // FIXME: Add missing impl.
    return Err(Error::NotSupported);
  }

  fn execute(&self, _sql: impl AsRef<str>, _params: impl Params) -> Result<usize, Error> {
    // FIXME: Add missing impl.
    return Err(Error::NotSupported);
  }

  fn execute_batch(&self, _sql: impl AsRef<str>) -> Result<(), Error> {
    // FIXME: Add missing impl.
    return Err(Error::NotSupported);
  }
}

impl<'a> SyncTransactionTrait for postgres::Transaction<'a> {
  fn commit(self) -> Result<(), Error> {
    return Ok(self.commit()?);
  }

  fn rollback(self) -> Result<(), Error> {
    return Ok(self.rollback()?);
  }

  fn expand_sql(
    &self,
    _sql: impl AsRef<str>,
    _params: impl Params,
  ) -> Result<Option<String>, Error> {
    return Err(Error::NotSupported);
  }
}
