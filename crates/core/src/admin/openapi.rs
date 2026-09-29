use axum::extract::Extension;
use parking_lot::Mutex;
use std::sync::Arc;
use utoipa::openapi::OpenApi;

use crate::admin::AdminError as Error;

#[derive(Clone, Default)]
pub(crate) struct OpenApiExtension {
  pub api: Arc<Mutex<Option<OpenApi>>>,
}

#[utoipa::path(
  get,
  path = "/openapi.json",
  tag = "admin",
  responses(
    (status = 200, description = "Success"),
  )
)]
pub async fn openapi_handler(openapi: Extension<OpenApiExtension>) -> Result<String, Error> {
  // NOTE: If memoizing Extension<OpenApi> turns out to be too much overhead but we still want the
  // WASM result. We could memoize WASM only. Rebuild OpenApiRouter for everything else here and
  // merge :shrug:. Feels overly complicated.
  //
  // let api = crate::openapi::build_api_definitions_from_config(.., /* include_admin= */ true);
  //
  // return api
  //   .to_pretty_json()
  //   .map_err(|err| Error::Other(err.to_string()));

  let lock = openapi.api.lock();
  let Some(api) = lock.as_ref() else {
    return Err(Error::Precondition("missing OpenApi defs".into()));
  };

  return api
    .to_pretty_json()
    .map_err(|err| Error::Other(err.to_string()));
}
