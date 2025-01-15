use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum_extra::protobuf::Protobuf;
use base64::prelude::*;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::config::proto::UpdateConfigRequest;
use crate::config::ConfigError;

pub async fn update_config_handler(
  State(state): State<AppState>,
  Protobuf(request): Protobuf<UpdateConfigRequest>,
) -> Result<impl IntoResponse, Error> {
  let Some(Ok(hash)) = request.hash.map(|h| BASE64_URL_SAFE.decode(h)) else {
    return Err(Error::Precondition("Missing hash".to_string()));
  };
  let Some(config) = request.config else {
    return Err(Error::Precondition("Missing config".to_string()));
  };

  let current_hash = state.get_config().hash();
  if current_hash.to_le_bytes() == *hash {
    state
      .validate_and_update_config(config, Some(current_hash))
      .await?;

    return Ok((StatusCode::OK, "Config updated"));
  }

  return Err(ConfigError::Update("Concurrent edit. Stale admin-UI cache?".to_string()).into());
}
