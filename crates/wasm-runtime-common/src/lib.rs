#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SqliteRequest {
  pub query: String,
  pub params: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SqliteResponse {
  Query { rows: Vec<Vec<serde_json::Value>> },
  Execute { rows_affected: usize },
  Error(String),
  TxBegin,
  TxCommit,
  TxRollback,
}
