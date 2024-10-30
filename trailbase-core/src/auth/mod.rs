use axum::{
  routing::{delete, get, post},
  Router,
};
use utoipa::OpenApi;

pub mod jwt;
pub mod user;

pub(crate) mod api;
pub(crate) mod oauth;
pub(crate) mod password;
pub(crate) mod tokens;
pub(crate) mod util;

mod error;
mod ui;

pub use api::reset_password::force_password_reset;
pub use error::AuthError;
pub use jwt::{JwtHelper, TokenClaims};
pub(crate) use ui::auth_ui_router;
pub use user::User;

#[derive(OpenApi)]
#[openapi(
  paths(
    api::login::login_handler,
    api::login::login_status_handler,
    api::token::auth_code_to_token_handler,
    api::logout::logout_handler,
    api::refresh::refresh_handler,
    api::register::register_user_handler,
    api::avatar::get_avatar_url_handler,
    api::delete::delete_handler,
    api::verify_email::verify_email_handler,
    api::verify_email::request_email_verification_handler,
    api::change_email::change_email_request_handler,
    api::change_email::change_email_confirm_handler,
    api::change_password::change_password_handler,
    api::reset_password::reset_password_request_handler,
    api::reset_password::reset_password_update_handler,
  ),
  components(schemas(
    api::login::LoginRequest,
    api::login::LoginResponse,
    api::login::LoginStatusResponse,
    api::token::TokenResponse,
    api::token::AuthCodeToTokenRequest,
    api::refresh::RefreshRequest,
    api::refresh::RefreshResponse,
    api::register::RegisterUserRequest,
    api::verify_email::EmailVerificationRequest,
    api::reset_password::ResetPasswordRequest,
    api::reset_password::ResetPasswordUpdateRequest,
    api::change_email::ChangeEmailRequest,
    api::change_password::ChangePasswordRequest,
  ))
)]
pub(super) struct AuthAPI;

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
  //    * change-email (TODO: CSRF: requires old email so only targeted),
  //    * delete-user (technically CSRF: however, currently DELETE method)
  //
  //  Avatar life-cycle: read+update are handled as record APIs.
  //
  //  TODO: We should have periodic task to:
  //   * expired auth, validate-email, reset-password codes.
  //   * vacuum expired pending registrations.
  return Router::new()
    // Sign-up new users.
    .route("/register", post(api::register::register_user_handler))
    // E-mail verification and change flows.
    .route(
      "/verify_email/trigger",
      get(api::verify_email::request_email_verification_handler),
    )
    .route(
      "/verify_email/confirm/:email_verification_code",
      get(api::verify_email::verify_email_handler),
    )
    .route(
      "/change_email/request",
      post(api::change_email::change_email_request_handler),
    )
    .route(
      "/change_email/confirm/:email_verification_code",
      get(api::change_email::change_email_confirm_handler),
    )
    // Password-reset flow.
    .route(
      "/reset_password/request",
      post(api::reset_password::reset_password_request_handler),
    )
    .route(
      "/reset_password/update/:password_reset_code",
      post(api::reset_password::reset_password_update_handler),
    )
    // Change password flow.
    .route(
      "/change_password",
      post(api::change_password::change_password_handler),
    )
    // Token refresh flow.
    .route("/refresh", post(api::refresh::refresh_handler))
    // Login
    .route("/login", post(api::login::login_handler))
    // Converts auth code (+pkce code verifier) to auth tokens
    .route("/token", post(api::token::auth_code_to_token_handler))
    // Login status (also let's one lift tokens from cookies).
    .route("/status", get(api::login::login_status_handler))
    // Logout [get]: deletes all sessions for the current user.
    .route("/logout", get(api::logout::logout_handler))
    // Logout [post]: deletes given session
    .route("/logout", post(api::logout::post_logout_handler))
    // Get a user's avatar.
    .route(
      "/avatar/:b64_user_id",
      get(api::avatar::get_avatar_url_handler),
    )
    // User delete.
    .route("/delete", delete(api::delete::delete_handler))
    // OAuth flows: list providers, login+callback
    .nest("/oauth", oauth::oauth_router());
}

/// Replicating minimal functionality of the above main router in case the admin dash is routed
/// from a different port to prevent cross-origin requests.
pub(super) fn admin_auth_router() -> Router<crate::AppState> {
  return Router::new()
    .route("/login", post(api::login::login_handler))
    .route("/status", get(api::login::login_status_handler))
    .route("/logout", get(api::logout::logout_handler));
}

#[cfg(test)]
mod auth_test;
