use axum::extract::State;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::openapi::build_api_definitions_from_state;

#[utoipa::path(
  get,
  path = "/openapi.json",
  tag = "admin",
  responses(
    (status = 200, description = "Success"),
  )
)]
pub async fn openapi_handler(State(state): State<AppState>) -> Result<String, Error> {
  let api = build_api_definitions_from_state(&state, /* include_admin= */ true);

  return api
    .to_pretty_json()
    .map_err(|err| Error::Other(err.to_string()));
}
