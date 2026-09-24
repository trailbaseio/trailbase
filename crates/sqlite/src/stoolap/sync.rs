use std::sync::Arc;

use crate::error::Error;
use crate::params::Params;
use crate::rows::{Row, Rows};
use crate::stoolap::value::map_params;
use crate::traits::SyncConnection as SyncConnectionTrait;
use crate::traits::SyncTransaction as SyncTransactionTrait;
use crate::r#type::ConnectionType;
use crate::value::Value;

impl SyncConnectionTrait for stoolap::Database {
  fn connection_type(&self) -> ConnectionType {
    return ConnectionType::Stoolap;
  }

  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    return Err(Error::NotImplemented);
  }

  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    return Err(Error::NotImplemented);
  }

  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    let params = map_params(params)?;
    return Ok(stoolap::Database::execute(&self, sql.as_ref(), params)? as usize);
  }

  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    // Is this supported?
    return Err(Error::NotImplemented);
  }
}

impl SyncConnectionTrait for stoolap::api::Transaction {
  fn connection_type(&self) -> ConnectionType {
    return ConnectionType::Stoolap;
  }

  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    return Err(Error::NotImplemented);
  }

  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    return Err(Error::NotImplemented);
  }

  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    return Err(Error::NotImplemented);
  }

  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    return Err(Error::NotImplemented);
  }
}

impl SyncTransactionTrait for stoolap::api::Transaction {
  fn commit(mut self) -> Result<(), Error> {
    return Ok(stoolap::api::Transaction::commit(&mut self)?);
  }

  fn rollback(mut self) -> Result<(), Error> {
    return Ok(stoolap::api::Transaction::rollback(&mut self)?);
  }

  fn expand_sql(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<String>, Error> {
    return Err(Error::NotImplemented);
  }
}
