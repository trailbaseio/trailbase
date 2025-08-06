pub(crate) mod provider;
pub(crate) mod providers;

mod callback;
mod list_providers;
mod login;
mod state;

#[cfg(test)]
mod oauth_test;

use axum::Router;
use axum::routing::get;
use utoipa::OpenApi;

pub(crate) use provider::{OAuthClientSettings, OAuthProvider, OAuthUser};

use crate::AppState;

#[derive(OpenApi)]
#[openapi(paths(
  list_providers::list_configured_providers_handler,
  login::login_with_external_auth_provider,
  callback::callback_from_external_auth_provider,
))]
pub(super) struct OAuthApi;

pub fn oauth_router() -> Router<AppState> {
  Router::new()
    .route(
      "/providers",
      get(list_providers::list_configured_providers_handler),
    )
    .route(
      "/{provider}/login",
      get(login::login_with_external_auth_provider),
    )
    .route(
      "/{provider}/callback",
      get(callback::callback_from_external_auth_provider),
    )
}
