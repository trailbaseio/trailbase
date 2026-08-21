use axum::extract::Extension;
use utoipa::openapi::OpenApi;

use crate::admin::AdminError as Error;

#[utoipa::path(
  get,
  path = "/openapi.json",
  tag = "admin",
  responses(
    (status = 200, description = "Success"),
  )
)]
pub async fn openapi_handler(openapi: Option<Extension<OpenApi>>) -> Result<String, Error> {
  // let api = crate::openapi::build_api_definitions_from_state(&state, /* include_admin= */ true);
  //
  // return api
  //   .to_pretty_json()
  //   .map_err(|err| Error::Other(err.to_string()));

  return openapi
    .unwrap_or_default()
    .to_pretty_json()
    .map_err(|err| Error::Other(err.to_string()));
}
