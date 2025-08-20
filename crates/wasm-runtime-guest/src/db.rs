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

// pub enum Value {
//   Null,
//   String(String),
//   Blob(Vec<u8>),
//   Integer(i64),
//   Real(f64),
// }
//
// impl TryFrom<serde_json::Value> for Value {
//   type Error = Error;
//
//   fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
//     return match value {
//       serde_json::Value::Null => Ok(Self::Null),
//       serde_json::Value::String(s) => Ok(Self::String(s)),
//       serde_json::Value::Object(mut map) => match map.remove("blob") {
//         Some(serde_json::Value::String(str)) => Ok(Value::Blob(BASE64_URL_SAFE.decode(&str)?)),
//         _ => Err(Error::UnexpectedType),
//       },
//       serde_json::Value::Number(n) => {
//         if let Some(n) = n.as_i64() {
//           Ok(Self::Integer(n))
//         } else if let Some(n) = n.as_u64() {
//           Ok(Value::Integer(n as i64))
//         } else if let Some(n) = n.as_f64() {
//           Ok(Value::Real(n))
//         } else {
//           Err(Error::NotANumber)
//         }
//       }
//       _ => Err(Error::UnexpectedType),
//     };
//   }
// }
//
// impl From<Value> for serde_json::Value {
//   fn from(value: Value) -> Self {
//     return match value {
//       Value::Null => Self::Null,
//       Value::String(s) => Self::String(s),
//       Value::Integer(i) => Self::Number(serde_json::Number::from(i)),
//       Value::Real(f) => Self::Number(serde_json::Number::from_f64(f).unwrap()),
//       Value::Blob(blob) => serde_json::json!({
//           "blob": BASE64_URL_SAFE.encode(blob)
//       }),
//     };
//   }
// }

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
