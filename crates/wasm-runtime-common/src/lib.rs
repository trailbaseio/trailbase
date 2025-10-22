#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use serde::{Deserialize, Serialize};
use trailbase_sqlvalue::SqlValue;
use ts_rs::TS;

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct SqliteRequest {
  pub query: String,
  pub params: Vec<SqlValue>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub enum SqliteResponse {
  Query { rows: Vec<Vec<SqlValue>> },
  Execute { rows_affected: usize },
  Error(String),
  TxBegin,
  TxCommit,
  TxRollback,
}

/// Used to pass extra information from host to guest via an HTTP request header "__context".
#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct HttpContext {
  pub kind: HttpContextKind,
  pub registered_path: String,
  pub path_params: Vec<(String, String)>,
  pub user: Option<HttpContextUser>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub enum HttpContextKind {
  /// An incoming http request.
  Http,
  /// An incoming job request.
  Job,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub struct HttpContextUser {
  /// Url-safe Base64 encoded id of the current user.
  pub id: String,
  /// E-mail of the current user.
  pub email: String,
  /// The "expected" CSRF token as included in the auth token claims [User] was constructed from.
  pub csrf_token: String,
}
