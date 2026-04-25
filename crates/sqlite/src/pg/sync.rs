use crate::error::Error;
use crate::params::Params;
use crate::rows::{Row, Rows};
use crate::traits::SyncConnection as SyncConnectionTrait;

impl SyncConnectionTrait for postgres::Client {
  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&self, _sql: impl AsRef<str>, _params: impl Params) -> Result<Option<Row>, Error> {
    return Err(Error::NotSupported);
  }

  fn query_rows(&self, _sql: impl AsRef<str>, _params: impl Params) -> Result<Rows, Error> {
    return Err(Error::NotSupported);
  }

  fn execute(&self, _sql: impl AsRef<str>, _params: impl Params) -> Result<usize, Error> {
    return Err(Error::NotSupported);
  }

  fn execute_batch(&self, _sql: impl AsRef<str>) -> Result<(), Error> {
    return Err(Error::NotSupported);
  }
}
