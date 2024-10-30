use axum::{extract::State, Json};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::table_metadata::sqlite3_parse_into_statements;

#[derive(Debug, Deserialize, Serialize, TS)]
pub enum Mode {
  Expression,
  Statement,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct ParseRequest {
  query: String,
  mode: Option<Mode>,
  // NOTE: We could probably be more specific for access checks setting up _REQ_, _ROW_, _USER_
  // appropriately.
  // create_access: bool,
  // read_access: bool,
  // update_access: bool,
  // delete_access: bool,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct ParseResponse {
  ok: bool,
  message: Option<String>,
}

pub async fn parse_handler(
  State(_state): State<AppState>,
  Json(request): Json<ParseRequest>,
) -> Result<Json<ParseResponse>, Error> {
  let query = String::from_utf8_lossy(&BASE64_URL_SAFE.decode(request.query)?).to_string();

  let result = match request.mode.unwrap_or(Mode::Expression) {
    Mode::Statement => sqlite3_parse_into_statements(&query),
    Mode::Expression => sqlite3_parse_into_statements(&format!("SELECT ({query})")),
  };

  return match result.err() {
    None => Ok(Json(ParseResponse {
      ok: true,
      message: None,
    })),
    Some(err) => Ok(Json(ParseResponse {
      ok: false,
      message: Some(err.to_string()),
    })),
  };
}
