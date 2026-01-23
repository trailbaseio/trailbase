use trailbase_sqlvalue::{Blob, DecodeError, SqlValue};

use crate::wit::trailbase::database::sqlite::{
  execute as call_execute, query as call_query, tx_begin, tx_commit, tx_execute, tx_query,
  tx_rollback,
};

pub use crate::wit::trailbase::database::sqlite::{TxError, Value};
pub use trailbase_wasm_common::{SqliteRequest, SqliteResponse};

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
    if !self.committed
      && let Err(err) = tx_rollback()
    {
      log::warn!("TX rollback failed: {err}");
    }
  }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Unexpected Type")]
  UnexpectedType,
  #[error("Not a Number")]
  NotANumber,
  #[error("Decoding")]
  Decoding(#[from] DecodeError),
  #[error("Other: {0}")]
  Other(String),
}

pub async fn query(
  query: impl std::string::ToString,
  params: impl Into<Vec<Value>>,
) -> Result<Vec<Vec<Value>>, Error> {
  return call_query(query.to_string(), params.into())
    .await
    .map_err(|err| Error::Other(err.to_string()));
}

pub async fn execute(
  query: impl std::string::ToString,
  params: impl Into<Vec<Value>>,
) -> Result<u64, Error> {
  return call_execute(query.to_string(), params.into())
    .await
    .map_err(|err| Error::Other(err.to_string()));
}

// fn from_json_value(value: serde_json::Value) -> Result<Value, Error> {
//   return match value {
//     serde_json::Value::Null => Ok(Value::Null),
//     serde_json::Value::String(s) => Ok(Value::Text(s)),
//     serde_json::Value::Object(mut map) => match map.remove("blob") {
//       Some(serde_json::Value::String(str)) => Ok(Value::Blob(BASE64_URL_SAFE.decode(&str)?)),
//       _ => Err(Error::UnexpectedType),
//     },
//     serde_json::Value::Number(n) => {
//       if let Some(n) = n.as_i64() {
//         Ok(Value::Integer(n))
//       } else if let Some(n) = n.as_u64() {
//         Ok(Value::Integer(n as i64))
//       } else if let Some(n) = n.as_f64() {
//         Ok(Value::Real(n))
//       } else {
//         Err(Error::NotANumber)
//       }
//     }
//     _ => Err(Error::UnexpectedType),
//   };
// }

// fn from_sql_value(value: SqlValue) -> Result<Value, DecodeError> {
//   return match value {
//     SqlValue::Null => Ok(Value::Null),
//     SqlValue::Integer(v) => Ok(Value::Integer(v)),
//     SqlValue::Real(v) => Ok(Value::Real(v)),
//     SqlValue::Text(v) => Ok(Value::Text(v)),
//     SqlValue::Blob(v) => Ok(Value::Blob(v.into_bytes()?)),
//   };
// }

// #[derive(Serialize)]
// struct Blob {
//   blob: String,
// }
//
// impl serde::ser::Serialize for Value {
//   fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//   where
//     S: serde::ser::Serializer,
//   {
//     return match self {
//       Value::Null => serializer.serialize_unit(),
//       Value::Text(s) => serializer.serialize_str(s),
//       Value::Integer(i) => serializer.serialize_i64(*i),
//       Value::Real(f) => serializer.serialize_f64(*f),
//       Value::Blob(blob) => serializer.serialize_some(&Blob {
//         blob: BASE64_URL_SAFE.encode(blob),
//       }),
//     };
//   }
// }

pub fn to_sql_value(value: Value) -> SqlValue {
  return match value {
    Value::Null => SqlValue::Null,
    Value::Text(s) => SqlValue::Text(s),
    Value::Integer(i) => SqlValue::Integer(i),
    Value::Real(f) => SqlValue::Real(f),
    Value::Blob(b) => SqlValue::Blob(Blob::Array(b)),
  };
}

// pub fn to_json_value(value: Value) -> serde_json::Value {
//   return match value {
//     Value::Null => serde_json::Value::Null,
//     Value::Text(s) => serde_json::Value::String(s),
//     Value::Integer(i) => serde_json::Value::Number(serde_json::Number::from(i)),
//     Value::Real(f) => match serde_json::Number::from_f64(f) {
//       Some(n) => serde_json::Value::Number(n),
//       None => serde_json::Value::Null,
//     },
//     Value::Blob(blob) => serde_json::json!({
//         "blob": BASE64_URL_SAFE.encode(blob)
//     }),
//   };
// }
