use crate::error::Error;
use crate::params::Params;
use crate::rows::{Row, Rows};
use crate::sqlite::lock::ArcLockGuard;
use crate::sqlite::sync::{execute, execute_batch, query_row, query_rows};
use crate::traits::{SyncConnection, SyncTransaction};
use crate::r#type::ConnectionType;

impl<'a> SyncConnection for rusqlite::Transaction<'a> {
  fn connection_type(&self) -> ConnectionType {
    return ConnectionType::Sqlite;
  }

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
  fn connection_type(&self) -> ConnectionType {
    return ConnectionType::Sqlite;
  }

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

self_cell::self_cell!(
  struct OwnedTxImpl {
    owner: self_cell::MutBorrow<ArcLockGuard>,

    #[covariant]
    dependent: Transaction,
  }
);

pub struct OwnedTx(OwnedTxImpl);

unsafe impl Send for OwnedTx {}

impl OwnedTx {
  pub fn new(lock: ArcLockGuard) -> Result<Self, Error> {
    return Ok(Self(OwnedTxImpl::try_new(
      self_cell::MutBorrow::new(lock),
      |owner| -> Result<Transaction, Error> {
        return Ok(Transaction {
          tx: owner.borrow_mut().transaction()?,
        });
      },
    )?));
  }
}

impl<'a> SyncConnection for OwnedTx {
  fn connection_type(&self) -> ConnectionType {
    return ConnectionType::Sqlite;
  }

  // Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error> {
    return self
      .0
      .with_dependent_mut(|_lock, tx| tx.query_row(sql, params));
  }

  fn query_rows(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error> {
    return self
      .0
      .with_dependent_mut(|_lock, tx| tx.query_rows(sql, params));
  }

  fn execute(&mut self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error> {
    return self
      .0
      .with_dependent_mut(|_lock, tx| tx.execute(sql, params));
  }

  fn execute_batch(&mut self, sql: impl AsRef<str>) -> Result<(), Error> {
    return self.0.with_dependent_mut(|_lock, tx| tx.execute_batch(sql));
  }
}

impl SyncTransaction for OwnedTx {
  fn commit(mut self) -> Result<(), Error> {
    // NOTE: this is the same as `tx.commit()` just w/o consuming.
    self
      .0
      .with_dependent_mut(|_lock, tx| tx.execute_batch("COMMIT"))?;
    return Ok(());
  }

  fn rollback(mut self) -> Result<(), Error> {
    // NOTE: this is the same as `tx.rollback()` just w/o consuming.
    self
      .0
      .with_dependent_mut(|_lock, tx| tx.execute_batch("ROLLBACK"))?;
    return Ok(());
  }

  fn expand_sql(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<String>, Error> {
    return self
      .0
      .with_dependent(|_lock, tx| tx.expand_sql(sql, params));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::sqlite::connection::Connection;

  #[test]
  fn test_owned_tx() {
    let conn = Connection::open_in_memory().unwrap();

    let lock = conn
      .try_write_arc_lock_for(tokio::time::Duration::from_micros(100))
      .unwrap();

    let mut tx = OwnedTx::new(lock).unwrap();
    tx.execute("CREATE TABLE foo (id INTEGER);", ()).unwrap();
    tx.rollback().unwrap();
  }
}
