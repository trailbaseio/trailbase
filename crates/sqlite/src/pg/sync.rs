use std::sync::Arc;

use postgres::fallible_iterator::FallibleIterator;

use crate::error::Error;
use crate::params::Params;
use crate::pg::util::{PgStatement, columns, from_row, from_rows};
use crate::rows::{Row, Rows};
use crate::traits::SyncConnection as SyncConnectionTrait;
use crate::traits::SyncTransaction as SyncTransactionTrait;
use crate::r#type::ConnectionType;
use crate::value::Value;

impl SyncConnectionTrait for postgres::Client {
  fn connection_type(&self) -> ConnectionType {
    return ConnectionType::Pg;
  }

  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    let (sql, params) = PgStatement::new(sql.as_ref())?.bind(params)?;
    let mut row_iter = self.query_raw(&sql, &params)?;

    if let Some(row) = row_iter.next()? {
      return Ok(Some(from_row(&row, Arc::new(columns(&row)))?));
    }

    return Ok(None);
  }

  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    let (sql, params) = PgStatement::new(sql.as_ref())?.bind(params)?;
    let row_iter = self.query_raw(&sql, &params)?;
    return from_rows(row_iter);
  }

  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    let (sql, params) = PgStatement::new(sql.as_ref())?.bind(params)?;
    let mut row_iter = self.query_raw(&sql, params)?;
    // Actually execute query.
    if row_iter.next()?.is_some() {
      return Err(Error::ExecuteReturnedResults);
    }
    return Ok(row_iter.rows_affected().unwrap_or_default() as usize);
  }

  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    return Ok(self.batch_execute(sql.as_ref())?);
  }
}

impl<'a> SyncConnectionTrait for postgres::Transaction<'a> {
  fn connection_type(&self) -> ConnectionType {
    return ConnectionType::Pg;
  }

  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    let (sql, params) = PgStatement::new(sql.as_ref())?.bind(params)?;
    let mut row_iter = self.query_raw(&sql, &params)?;

    if let Some(row) = row_iter.next()? {
      return Ok(Some(from_row(&row, Arc::new(columns(&row)))?));
    }

    return Ok(None);
  }

  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    let (sql, params) = PgStatement::new(sql.as_ref())?.bind(params)?;
    let row_iter = self.query_raw(&sql, &params)?;
    return from_rows(row_iter);
  }

  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    let (sql, params) = PgStatement::new(sql.as_ref())?.bind(params)?;
    let row_iter = self.query_raw(&sql, params)?;
    return Ok(row_iter.rows_affected().unwrap_or_default() as usize);
  }

  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    return Ok(self.batch_execute(sql.as_ref())?);
  }
}

impl<'a> SyncTransactionTrait for postgres::Transaction<'a> {
  fn commit(self) -> Result<(), Error> {
    return Ok(self.commit()?);
  }

  fn rollback(self) -> Result<(), Error> {
    return Ok(self.rollback()?);
  }

  fn expand_sql(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<String>, Error> {
    let (mut sql, params) = PgStatement::new(sql.as_ref())?.bind(params)?;

    for (idx, param) in params.into_iter().enumerate() {
      let param_string = match param {
        Value::Null => "NULL",
        Value::Blob(blob) => {
          let hex_string: String = blob.iter().map(|b| format!("{:02x}", b)).collect();

          // PG expects something like: `'\xDEADBEEF'::bytea`.
          &format!("'\\x{hex_string}'::bytea")
        }
        Value::Real(v) => &format!("{v}"),
        Value::Integer(v) => &format!("{v}"),
        Value::Text(t) => &format!("'{t}'"),
      };

      sql = sql.replace(&format!("${}", idx + 1), param_string)
    }

    return Ok(Some(sql));
  }
}
