use crate::error::Error;
use crate::params::Params;
use crate::rows::{Rc, Row, Rows};
use crate::stoolap::value::map_params;
use crate::traits::SyncConnection as SyncConnectionTrait;
use crate::traits::SyncTransaction as SyncTransactionTrait;
use crate::r#type::ConnectionType;

impl SyncConnectionTrait for stoolap::Database {
  fn connection_type(&self) -> ConnectionType {
    return ConnectionType::Stoolap;
  }

  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    let params = map_params(params)?;
    let mut rows = self.query(sql.as_ref(), params)?;

    if let Some(row) = rows.next() {
      let row = row?;
      let columns = crate::stoolap::value::columns(&row);
      return Ok(Some(crate::stoolap::value::from_row(
        row,
        Rc::new(columns),
      )?));
    }

    return Ok(None);
  }

  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    let params = map_params(params)?;
    let rows = self.query(sql.as_ref(), params)?;
    return crate::stoolap::value::from_rows(rows);
  }

  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    let params = map_params(params)?;
    return Ok(stoolap::Database::execute(&self, sql.as_ref(), params)? as usize);
  }

  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    // Stoolap's execute can execute multiple statements.
    stoolap::Database::execute(&self, sql.as_ref(), ())?;
    return Ok(());
  }
}

impl SyncConnectionTrait for stoolap::api::Transaction {
  fn connection_type(&self) -> ConnectionType {
    return ConnectionType::Stoolap;
  }

  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    let params = map_params(params)?;
    let mut rows = self.query(sql.as_ref(), params)?;

    if let Some(row) = rows.next() {
      let row = row?;
      let columns = crate::stoolap::value::columns(&row);
      return Ok(Some(crate::stoolap::value::from_row(
        row,
        Rc::new(columns),
      )?));
    }

    return Ok(None);
  }

  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    let params = map_params(params)?;
    let rows = self.query(sql.as_ref(), params)?;
    return crate::stoolap::value::from_rows(rows);
  }

  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    let params = map_params(params)?;
    return Ok(self.execute(sql.as_ref(), params)? as usize);
  }

  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    self.execute(sql.as_ref(), ())?;
    return Ok(());
  }
}

impl SyncTransactionTrait for stoolap::api::Transaction {
  fn commit(mut self) -> Result<(), Error> {
    return Ok(stoolap::api::Transaction::commit(&mut self)?);
  }

  fn rollback(mut self) -> Result<(), Error> {
    return Ok(stoolap::api::Transaction::rollback(&mut self)?);
  }

  fn expand_sql(
    &self,
    _sql: impl AsRef<str>,
    params: impl Params,
  ) -> Result<Option<String>, Error> {
    return Err(Error::NotImplemented);
  }
}
