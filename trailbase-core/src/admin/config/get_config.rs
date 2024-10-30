use axum::extract::State;
use axum_extra::protobuf::Protobuf;
use base64::prelude::*;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::config::proto::GetConfigResponse;

pub async fn get_config_handler(
  State(state): State<AppState>,
) -> Result<Protobuf<GetConfigResponse>, Error> {
  let config = state.get_config();
  let hash = config.hash();

  // NOTE: We used to strip the secrets to avoid exposing them in the admin dashboard. This would
  // be mostly relevant if no TLS (plain text transmission) or if we wanted to have less privileged
  // dashboard users than admins.
  // We went back on this for now, since this requires very complicated merging. For example, an
  // oauth provider is already configured and an admin adds another one. You get back:
  //
  //     [
  //      { provider_id: X, client_id: "old" },
  //      { provider_id: Y, client_id: "new", client_secret: "new_secret" },
  //     ]
  //
  //  which fails validation because "old" is missing the "secret". We'd have to merge secrets back
  //  before validation on entries, which haven't been removed, ... and this true for all secrets.
  //
  // let (stripped, _secrets) = strip_secrets(&config)?;

  return Ok(Protobuf(GetConfigResponse {
    config: Some(config),
    hash: Some(BASE64_URL_SAFE.encode(hash.to_le_bytes())),
  }));
}
