use trailbase_sqlvalue::{Blob, DecodeError, SqlValue};
use wstd::http::body::IntoBody;
use wstd::http::{Client, Request};

use crate::wit::trailbase::database::sqlite::Transaction as WasiTransaction;

pub use crate::wit::trailbase::database::sqlite::{TxError, Value};
pub use trailbase_wasm_common::{SqliteRequest, SqliteResponse};

/// Escapes arbitrary strings as a safe SQL string literal, e.g. 'foo'.
pub fn escape(s: impl AsRef<str>) -> String {
  let input = s.as_ref();
  let mut buf = String::with_capacity(input.len() * 2 + 2);

  buf.push('\'');
  for b in input.chars() {
    match b {
      '\'' => buf.push_str("''"),
      // Not strictly an injection risk, just being defensive here for downstream consumers.
      '\0' => buf.push_str("\\0"),
      _ => buf.push(b),
    }
  }
  buf.push('\'');

  return buf;
}

pub struct Transaction {
  tx: WasiTransaction,
  committed: bool,
}

impl Transaction {
  pub fn begin() -> Result<Self, TxError> {
    let tx = WasiTransaction::new();
    tx.begin()?;
    return Ok(Self {
      tx,
      committed: false,
    });
  }

  pub fn query(&mut self, query: &str, params: &[Value]) -> Result<Vec<Vec<Value>>, TxError> {
    return self.tx.query(query, params);
  }

  pub fn execute(&mut self, query: &str, params: &[Value]) -> Result<u64, TxError> {
    return self.tx.execute(query, params);
  }

  pub fn commit(&mut self) -> Result<(), TxError> {
    if !self.committed {
      self.committed = true;
      self.tx.commit()?;
    }
    return Ok(());
  }
}

impl Drop for Transaction {
  fn drop(&mut self) {
    if !self.committed
      && let Err(err) = self.tx.rollback()
    {
      log::warn!("TX rollback failed: {err}");
    }
  }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
  #[error("Unexpected Type: {0}")]
  UnexpectedType(Box<dyn std::error::Error>),
  #[error("Decoding: {0}")]
  Decoding(Box<dyn std::error::Error>),
  #[error("Other: {0}")]
  Other(Box<dyn std::error::Error>),
}

impl From<DecodeError> for Error {
  fn from(err: DecodeError) -> Self {
    return Self::Decoding(err.into());
  }
}

impl From<serde_json::Error> for Error {
  fn from(err: serde_json::Error) -> Self {
    return Self::Decoding(err.into());
  }
}

pub async fn query(
  query: impl std::string::ToString,
  params: impl Into<Vec<Value>>,
) -> Result<Vec<Vec<Value>>, Error> {
  let r = SqliteRequest {
    query: query.to_string(),
    params: params.into().into_iter().map(to_sql_value).collect(),
  };
  let request = Request::builder()
    .uri("http://__sqlite/query")
    .method("POST")
    .body(serde_json::to_vec(&r)?.into_body())
    .map_err(|err| Error::Other(err.into()))?;

  let client = Client::new();
  let (_parts, mut body) = client
    .send(request)
    .await
    .map_err(|err| Error::Other(err.into()))?
    .into_parts();

  let bytes = body.bytes().await.map_err(|err| Error::Other(err.into()))?;

  return match serde_json::from_slice(&bytes) {
    Ok(SqliteResponse::Query { rows }) => Ok(
      rows
        .into_iter()
        .map(|row| {
          row
            .into_iter()
            .map(from_sql_value)
            .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?,
    ),
    Ok(SqliteResponse::Error(err)) => Err(Error::Other(err.into())),
    Ok(resp) => Err(Error::UnexpectedType(
      format!("Expected QueryResponse, got: {resp:?}").into(),
    )),
    Err(err) => Err(Error::Other(err.into())),
  };
}

pub async fn execute(
  query: impl std::string::ToString,
  params: impl Into<Vec<Value>>,
) -> Result<usize, Error> {
  let r = SqliteRequest {
    query: query.to_string(),
    params: params.into().into_iter().map(to_sql_value).collect(),
  };
  let request = Request::builder()
    .uri("http://__sqlite/execute")
    .method("POST")
    .body(serde_json::to_vec(&r)?.into_body())
    .map_err(|err| Error::Other(err.into()))?;

  let client = Client::new();
  let (_parts, mut body) = client
    .send(request)
    .await
    .map_err(|err| Error::Other(err.into()))?
    .into_parts();

  let bytes = body.bytes().await.map_err(|err| Error::Other(err.into()))?;

  return match serde_json::from_slice(&bytes) {
    Ok(SqliteResponse::Execute { rows_affected }) => Ok(rows_affected),
    Ok(SqliteResponse::Error(err)) => Err(Error::Other(err.into())),
    Ok(resp) => Err(Error::UnexpectedType(
      format!("Expected ExecuteResponse, got: {resp:?}").into(),
    )),
    Err(err) => Err(Error::Other(err.into())),
  };
}

fn from_sql_value(value: SqlValue) -> Result<Value, DecodeError> {
  return match value {
    SqlValue::Null => Ok(Value::Null),
    SqlValue::Integer(v) => Ok(Value::Integer(v)),
    SqlValue::Real(v) => Ok(Value::Real(v)),
    SqlValue::Text(v) => Ok(Value::Text(v)),
    SqlValue::Blob(v) => Ok(Value::Blob(v.into_bytes()?)),
  };
}

pub fn to_sql_value(value: Value) -> SqlValue {
  return match value {
    Value::Null => SqlValue::Null,
    Value::Text(s) => SqlValue::Text(s),
    Value::Integer(i) => SqlValue::Integer(i),
    Value::Real(f) => SqlValue::Real(f),
    Value::Blob(b) => SqlValue::Blob(Blob::Array(b)),
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn escape_test() {
    assert_eq!("'foo'", escape("foo"));
    assert_eq!("'f''oo'", escape("f'oo"));
    assert_eq!("'foo\\0more'", escape("foo\0more"));
  }
}
