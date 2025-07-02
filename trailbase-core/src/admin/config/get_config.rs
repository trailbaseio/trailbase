use axum::extract::State;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::config::{
  proto::{GetConfigResponse, hash_config},
  redact_secrets,
};
use crate::extract::protobuf::Protobuf;

pub async fn get_config_handler(
  State(state): State<AppState>,
) -> Result<Protobuf<GetConfigResponse>, Error> {
  let config = state.get_config();
  let hash = hash_config(&config);

  let (stripped, _secrets) = redact_secrets(&config)?;

  return Ok(Protobuf(GetConfigResponse {
    config: Some(stripped),
    hash: Some(hash),
  }));
}
