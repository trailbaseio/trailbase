use base64::prelude::*;
use wstd::http::body::IntoBody;
use wstd::http::{Client, Request};

use crate::wit::trailbase::runtime::host_endpoint::{
  tx_begin, tx_commit, tx_execute, tx_query, tx_rollback,
};

pub use crate::wit::trailbase::runtime::host_endpoint::{TxError, Value};
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
    if !self.committed {
      if let Err(err) = tx_rollback() {
        log::warn!("TX rollback failed: {err}");
      }
    }
  }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Sqlite: {0}")]
  Sqlite(String),
  #[error("Unsexpected Type")]
  UnexpectedType,
  #[error("Not a Number")]
  NotANumber,
  #[error("Decoding")]
  Decording(#[from] base64::DecodeError),
}

pub async fn query(query: String, params: Vec<Value>) -> Result<Vec<Vec<Value>>, Error> {
  let r = SqliteRequest {
    query,
    params: params
      .into_iter()
      .map(to_json_value)
      .collect::<Result<Vec<_>, _>>()?,
  };
  let request = Request::builder()
    .uri("http://__sqlite/query")
    .method("POST")
    .body(serde_json::to_vec(&r).expect("serialization").into_body());

  let client = Client::new();
  let (_parts, mut body) = client
    .send(request.unwrap())
    .await
    .expect("foo")
    .into_parts();

  let bytes = body.bytes().await.expect("baz");
  return match serde_json::from_slice(&bytes) {
    Ok(SqliteResponse::Query { rows }) => Ok(
      rows
        .into_iter()
        .map(|row| {
          row
            .into_iter()
            .map(from_json_value)
            .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?,
    ),
    Ok(_) => Err(Error::Sqlite("Unexpected response type".to_string())),
    Err(err) => Err(Error::Sqlite(err.to_string())),
  };
}

pub async fn execute(query: String, params: Vec<Value>) -> Result<usize, Error> {
  let r = SqliteRequest {
    query,
    params: params
      .into_iter()
      .map(to_json_value)
      .collect::<Result<Vec<_>, _>>()?,
  };
  let request = Request::builder()
    .uri("http://__sqlite/execute")
    .method("POST")
    .body(serde_json::to_vec(&r).expect("serialization").into_body());

  let client = Client::new();
  let (_parts, mut body) = client
    .send(request.unwrap())
    .await
    .expect("foo")
    .into_parts();

  let bytes = body.bytes().await.expect("baz");
  return match serde_json::from_slice(&bytes) {
    Ok(SqliteResponse::Execute { rows_affected }) => Ok(rows_affected),
    Ok(_) => Err(Error::Sqlite("Unexpected response type".to_string())),
    Err(err) => Err(Error::Sqlite(err.to_string())),
  };
}

fn from_json_value(value: serde_json::Value) -> Result<Value, Error> {
  return match value {
    serde_json::Value::Null => Ok(Value::Null),
    serde_json::Value::String(s) => Ok(Value::Text(s)),
    serde_json::Value::Object(mut map) => match map.remove("blob") {
      Some(serde_json::Value::String(str)) => Ok(Value::Blob(BASE64_URL_SAFE.decode(&str)?)),
      _ => Err(Error::UnexpectedType),
    },
    serde_json::Value::Number(n) => {
      if let Some(n) = n.as_i64() {
        Ok(Value::Integer(n))
      } else if let Some(n) = n.as_u64() {
        Ok(Value::Integer(n as i64))
      } else if let Some(n) = n.as_f64() {
        Ok(Value::Real(n))
      } else {
        Err(Error::NotANumber)
      }
    }
    _ => Err(Error::UnexpectedType),
  };
}

fn to_json_value(value: Value) -> Result<serde_json::Value, Error> {
  return match value {
    Value::Null => Ok(serde_json::Value::Null),
    Value::Text(s) => Ok(serde_json::Value::String(s)),
    Value::Integer(i) => Ok(serde_json::Value::Number(serde_json::Number::from(i))),
    Value::Real(f) => Ok(serde_json::Value::Number(
      serde_json::Number::from_f64(f).unwrap(),
    )),
    Value::Blob(blob) => Ok(serde_json::json!({
        "blob": BASE64_URL_SAFE.encode(blob)
    })),
  };
}
