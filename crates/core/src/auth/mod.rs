use utoipa_axum::router::OpenApiRouter;

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

pub use error::AuthError;
pub use jwt::{AuthTokenClaims, JwtHelper};
// pub(crate) use ui::auth_ui_router;
pub use user::User;

use crate::AppState;
use crate::config::proto;

/// Router for auth API endpoints, i.e. api/auth/v?/... .
pub(super) fn router(config: &proto::Config) -> OpenApiRouter<AppState> {
  // We support the following authentication flows:
  //
  //  * unauthed: register (anonymous + normal), login, get-avatar-url
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
  //    * promote-anonymous.
  //
  //  Avatar life-cycle: read+update are handled as record APIs.

  // Using the utoipa integration, we can use the on-handler metadata as the
  // source of truth for registering the routes avoiding skew.
  // Inversely, using this macro ensures that the handlers do have metadata.
  use utoipa_axum::routes;

  let mut router = OpenApiRouter::new()
    .routes(routes!(api::register::register_user_handler))
    // E-mail verification and change flows.
    .routes(routes!(
      api::verify_email::request_email_verification_handler,
    ))
    .routes(routes!(api::verify_email::verify_email_handler))
    .routes(routes!(api::change_email::change_email_request_handler))
    .routes(routes!(api::change_email::change_email_confirm_handler))
    // Change username flow.
    .routes(routes!(api::change_username::change_username_handler))
    // // Password-reset flow.
    .routes(routes!(api::reset_password::reset_password_request_handler))
    .routes(routes!(api::reset_password::reset_password_update_handler))
    // Change password flow.
    .routes(routes!(api::change_password::change_password_handler))
    // Token refresh flow.
    .routes(routes!(api::refresh::refresh_handler))
    // Login
    .routes(routes!(api::login::login_handler))
    .routes(routes!(api::login::login_mfa_handler))
    // TOTP flow
    .routes(routes!(api::totp::register_totp_request_handler))
    .routes(routes!(api::totp::register_totp_confirm_handler))
    .routes(routes!(api::totp::unregister_totp_handler))
    // Converts auth code (+pkce code verifier) to auth tokens
    .routes(routes!(api::token::auth_code_to_token_handler))
    // Login status (also let's one lift tokens from cookies).
    .routes(routes!(api::status::login_status_handler))
    .routes(routes!(
      // Logout [get]: deletes all sessions for the current user.
      api::logout::logout_handler,
      // Logout [post]: deletes given session
      api::logout::post_logout_handler,
    ))
    // Get a user's avatar.
    .routes(routes!(api::avatar::get_avatar_handler))
    .routes(routes!(api::avatar::create_avatar_handler))
    .routes(routes!(api::avatar::delete_avatar_handler))
    // User delete.
    .routes(routes!(api::delete::delete_handler))
    // OAuth flows: list providers, login+callback
    .merge(oauth::oauth_router());

  if config.auth.enable_anonymous_signin() {
    router = router
      .routes(routes!(api::login_anonymous::login_anonymous_user_handler))
      .routes(routes!(
        api::promote_anonymous::promote_anonymous_user_handler
      ));
  }

  if config.auth.enable_otp_signin() {
    router = router
      // OTP flow
      .routes(routes!(api::otp::request_otp_handler))
      .routes(routes!(api::otp::login_otp_handler));
  }

  return router;
}

/// Replicating minimal functionality of the above main router in case the admin dash is routed
/// from a different port to prevent cross-origin requests.
pub(super) fn admin_auth_router() -> OpenApiRouter<AppState> {
  // Using the utoipa integration, we can use the on-handler metadata as the
  // source of truth for registering the routes avoiding skew.
  // Inversely, using this macro ensures that the handlers do have metadata.
  use utoipa_axum::routes;

  return OpenApiRouter::new()
    .routes(routes!(api::login::login_handler))
    .routes(routes!(api::status::login_status_handler))
    .routes(routes!(api::logout::logout_handler));
}

#[cfg(test)]
mod auth_test;
