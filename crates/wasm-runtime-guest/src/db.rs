pub use crate::wit::trailbase::runtime::host_endpoint::{TxError, Value};

use crate::wit::trailbase::runtime::host_endpoint::{
  tx_begin, tx_commit, tx_execute, tx_query, tx_rollback,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Not a Number")]
  NotANumber,
  #[error("Unexpected type")]
  UnexpectedType,
}

pub struct Transaction {
  committed: bool,
}

impl Transaction {
  pub fn begin() -> Result<Self, TxError> {
    tx_begin()?;
    return Ok(Self { committed: false });
  }

  pub fn query(&mut self, query: &str, params: &[Value]) -> Result<Vec<Vec<Value>>, TxError> {
    return tx_query(query, params);
  }

  pub fn execute(&mut self, query: &str, params: &[Value]) -> Result<u64, TxError> {
    return tx_execute(query, params);
  }

  pub fn commit(&mut self) -> Result<(), TxError> {
    if !self.committed {
      self.committed = true;
      tx_commit()?;
    }
    return Ok(());
  }
}

impl Drop for Transaction {
  fn drop(&mut self) {
    if !self.committed {
      if let Err(err) = tx_rollback() {
        log::warn!("TX rollback failed: {err}");
      }
    }
  }
}
