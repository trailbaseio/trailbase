use axum::extract::State;
use axum::http::{HeaderMap, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::config::proto::{GetConfigResponse, hash_config};
use crate::config::redact_secrets;
use crate::extract::protobuf::{Protobuf, Textproto};

#[utoipa::path(
  get,
  path = "/config",
  tag = "admin",
  responses(
    (status = 200, content_type = "application/x-protobuf", description = "config::GetConfigResponse protobuf"),
    (status = 200, content_type = "text/plain", description = "config::GetConfigResponse textproto"),
  )
)]
pub async fn get_config_handler(
  State(state): State<AppState>,
  headers: HeaderMap,
) -> Result<Response, Error> {
  let config = state.get_config();
  let hash = hash_config(&config);

  let (stripped, _secrets) = redact_secrets(&config)?;

  return match headers.get(CONTENT_TYPE) {
    Some(content_type) if content_type == "text/plain" => Ok(
      Textproto(GetConfigResponse {
        config: Some(stripped),
        hash: Some(hash),
      })
      .into_response(),
    ),
    _ => Ok(
      Protobuf(GetConfigResponse {
        config: Some(stripped),
        hash: Some(hash),
      })
      .into_response(),
    ),
  };
}
