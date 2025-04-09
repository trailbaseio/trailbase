use axum::{extract::State, Json};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use trailbase_schema::sqlite::sqlite3_parse_into_statement;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;

#[derive(Debug, Deserialize, PartialEq, Serialize, TS)]
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
  let mode = request.mode.unwrap_or(Mode::Expression);
  let decoded = String::from_utf8_lossy(&BASE64_URL_SAFE.decode(request.query)?).to_string();
  let query: String = match mode {
    Mode::Statement => decoded,
    Mode::Expression => format!("SELECT {decoded}"),
  };

  if let Err(err) = sqlite3_parse_into_statement(&query) {
    return Ok(Json(ParseResponse {
      ok: false,
      message: Some(err.to_string()),
    }));
  }

  return Ok(Json(ParseResponse {
    ok: true,
    message: None,
  }));
}
