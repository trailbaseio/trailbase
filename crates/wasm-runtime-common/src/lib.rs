#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use serde::{Deserialize, Serialize};
use trailbase_sqlvalue::SqlValue;
use ts_rs::TS;

pub mod manifest;

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
  // Host encodes with padded BASE64_URL_SAFE, but the guest's `is_admin` decodes
  // with URL_SAFE_NO_PAD which rejects trailing `=`. Strip on deserialize.
  //
  // FIXME: "failed to parse serde attribute" and why didn't this surface before?
  // We should always use padding.
  // #[serde(deserialize_with = "deserialize_trim_padding")]
  pub id: String,
  /// E-mail of the current user.
  pub email: Option<String>,
  /// Username of the current user.
  pub username: Option<String>,
  /// The "expected" CSRF token as included in the auth token claims [User] was constructed from.
  pub csrf_token: String,
}

fn deserialize_trim_padding<'de, D: serde::Deserializer<'de>>(
  deserializer: D,
) -> Result<String, D::Error> {
  Ok(
    String::deserialize(deserializer)?
      .trim_end_matches('=')
      .to_string(),
  )
}
