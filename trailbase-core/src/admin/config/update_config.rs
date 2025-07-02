use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::config::proto::{UpdateConfigRequest, Vault};
use crate::config::{merge_vault_and_env, redact_secrets};
use crate::extract::protobuf::Protobuf;

pub async fn update_config_handler(
  State(state): State<AppState>,
  Protobuf(request): Protobuf<UpdateConfigRequest>,
) -> Result<impl IntoResponse, Error> {
  let Some(hash) = request.hash else {
    return Err(Error::Precondition("Missing hash".to_string()));
  };
  let Some(config) = request.config else {
    return Err(Error::Precondition("Missing config".to_string()));
  };

  let current = state.get_config();
  let (_, secrets) = redact_secrets(&current)?;

  let merged = merge_vault_and_env(config, Vault { secrets })?;

  state.validate_and_update_config(merged, Some(hash)).await?;

  return Ok((StatusCode::OK, "Config updated"));
}
