mod callback;
mod list_providers;
mod login;
pub(crate) mod provider;
pub(crate) mod providers;
mod reqwest_client;
pub(crate) mod simple_provider;
mod state;

#[cfg(test)]
mod oauth_test;

use utoipa_axum::router::OpenApiRouter;

use crate::AppState;

pub(crate) use provider::{OAuthClientSettings, OAuthProvider, OAuthUser};
pub(crate) use reqwest_client::ReqwestClient;

pub fn oauth_router() -> OpenApiRouter<AppState> {
  // Using the utoipa integration, we can use the on-handler metadata as the
  // source of truth for registering the routes avoiding skew.
  // Inversely, using this macro ensures that the handlers do have metadata.
  use utoipa_axum::routes;

  return OpenApiRouter::new()
    .routes(routes!(list_providers::list_configured_providers_handler))
    .routes(routes!(login::login_with_external_auth_provider))
    .routes(routes!(callback::callback_from_external_auth_provider));
}
