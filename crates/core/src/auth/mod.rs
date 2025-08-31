use axum::{
  Router,
  routing::{delete, get, post},
};
use utoipa::OpenApi;

pub mod cli;
pub mod jwt;
pub mod user;

pub(crate) mod api;
pub(crate) mod login_params;
pub(crate) mod oauth;
pub(crate) mod options;
pub(crate) mod password;
pub(crate) mod tokens;
pub(crate) mod util;

mod error;
mod ui;

pub use error::AuthError;
pub use jwt::{JwtHelper, TokenClaims};
pub(crate) use ui::auth_ui_router;
pub use user::User;

use crate::constants::AUTH_API_PATH;

// NOTE: This import is needed to not mangle names in OpenAPI export.
use api::*;

#[derive(OpenApi)]
#[openapi(
  tags(
      (name = "auth", description = "Auth-related APIs"),
  ),
  paths(
    register::register_user_handler,
    verify_email::request_email_verification_handler,
    verify_email::verify_email_handler,
    change_email::change_email_request_handler,
    change_email::change_email_confirm_handler,
    reset_password::reset_password_request_handler,
    reset_password::reset_password_update_handler,
    change_password::change_password_handler,
    refresh::refresh_handler,
    login::login_handler,
    token::auth_code_to_token_handler,
    status::login_status_handler,
    logout::logout_handler,
    logout::post_logout_handler,
    avatar::get_avatar_handler,
    avatar::create_avatar_handler,
    avatar::delete_avatar_handler,
    delete::delete_handler,
  ),
  nest(
     (path = "/oauth", api = oauth::OAuthApi),
  ),
)]
pub(super) struct AuthApi;

/// Router for auth API endpoints, i.e. api/auth/v?/... .
pub(super) fn router() -> Router<crate::AppState> {
  // We support the following authentication flows:
  //
  //  * unauthed: register, login, get-avatar-url
  //  * unauthed + rate limited:
  //    * reset-password
  //    * verify-email (+retrigger)
  //  * authed:
  //    * get-login-status (no CSRF, no side-effect)
  //    * refresh-token (no CSRF, safe side-effect)
  //    * logout (no CSRF, safe side-effect)
  //    * change-password (no CSRF: requires old pass),
  //    * change-email (CSRF: requires old email so only targeted),
  //    * delete-user (technically CSRF: however, currently DELETE method)
  //
  //  Avatar life-cycle: read+update are handled as record APIs.
  //
  //  TODO: We should have periodic task to vacuum expired auth, validate-email, reset-password
  //  codes and pending registrations.
  return Router::new()
    // Sign-up new users.
    .route(
      &format!("/{AUTH_API_PATH}/register"),
      post(api::register::register_user_handler),
    )
    // E-mail verification and change flows.
    .route(
      &format!("/{AUTH_API_PATH}/verify_email/trigger"),
      get(api::verify_email::request_email_verification_handler),
    )
    .route(
      &format!("/{AUTH_API_PATH}/verify_email/confirm/{{email_verification_code}}"),
      get(api::verify_email::verify_email_handler),
    )
    .route(
      &format!("/{AUTH_API_PATH}/change_email/request"),
      post(api::change_email::change_email_request_handler),
    )
    .route(
      &format!("/{AUTH_API_PATH}/change_email/confirm/{{email_verification_code}}"),
      get(api::change_email::change_email_confirm_handler),
    )
    // Password-reset flow.
    .route(
      &format!("/{AUTH_API_PATH}/reset_password/request"),
      post(api::reset_password::reset_password_request_handler),
    )
    .route(
      &format!("/{AUTH_API_PATH}/reset_password/update"),
      post(api::reset_password::reset_password_update_handler),
    )
    // Change password flow.
    .route(
      &format!("/{AUTH_API_PATH}/change_password"),
      post(api::change_password::change_password_handler),
    )
    // Token refresh flow.
    .route(
      &format!("/{AUTH_API_PATH}/refresh"),
      post(api::refresh::refresh_handler),
    )
    // Login
    .route(
      &format!("/{AUTH_API_PATH}/login"),
      post(api::login::login_handler),
    )
    // Converts auth code (+pkce code verifier) to auth tokens
    .route(
      &format!("/{AUTH_API_PATH}/token"),
      post(api::token::auth_code_to_token_handler),
    )
    // Login status (also let's one lift tokens from cookies).
    .route(
      &format!("/{AUTH_API_PATH}/status"),
      get(api::status::login_status_handler),
    )
    // Logout [get]: deletes all sessions for the current user.
    .route(
      &format!("/{AUTH_API_PATH}/logout"),
      get(api::logout::logout_handler),
    )
    // Logout [post]: deletes given session
    .route(
      &format!("/{AUTH_API_PATH}/logout"),
      post(api::logout::post_logout_handler),
    )
    // Get a user's avatar.
    .route(
      &format!("/{AUTH_API_PATH}/avatar/{{b64_user_id}}"),
      get(api::avatar::get_avatar_handler),
    )
    .route(
      &format!("/{AUTH_API_PATH}/avatar"),
      post(api::avatar::create_avatar_handler),
    )
    .route(
      &format!("/{AUTH_API_PATH}/avatar"),
      delete(api::avatar::delete_avatar_handler),
    )
    // User delete.
    .route(
      &format!("/{AUTH_API_PATH}/delete"),
      delete(api::delete::delete_handler),
    )
    // OAuth flows: list providers, login+callback
    .nest(&format!("/{AUTH_API_PATH}/oauth"), oauth::oauth_router());
}

/// Replicating minimal functionality of the above main router in case the admin dash is routed
/// from a different port to prevent cross-origin requests.
pub(super) fn admin_auth_router() -> Router<crate::AppState> {
  return Router::new()
    .route(
      &format!("/{AUTH_API_PATH}/login"),
      post(api::login::login_handler),
    )
    .route(
      &format!("/{AUTH_API_PATH}/status"),
      get(api::status::login_status_handler),
    )
    .route(
      &format!("/{AUTH_API_PATH}/logout"),
      get(api::logout::logout_handler),
    );
}

#[cfg(test)]
mod auth_test;
