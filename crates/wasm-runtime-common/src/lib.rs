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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HttpContextUser {
  /// Url-safe Base64 encoded id of the current user.
  pub id: String,
  /// E-mail of the current user.
  pub email: String,
  /// The "expected" CSRF token as included in the auth token claims [User] was constructed from.
  pub csrf_token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HttpContext {
  pub registered_path: String,
  pub path_params: Vec<(String, String)>,
  pub user: Option<HttpContextUser>,
}
