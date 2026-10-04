pub mod cli;
pub mod jwt;
pub mod user;

pub(crate) mod api;
pub(crate) mod apple;
pub(crate) mod create_external_user;
pub(crate) mod login_params;
pub(crate) mod oauth;
pub(crate) mod options;
pub(crate) mod password;
pub(crate) mod tokens;
pub(crate) mod util;

mod error;

pub use api::verify_email::EMAIL_VERIFICATION_TTL;
pub use error::AuthError;
pub use jwt::{AuthTokenClaims, JwtHelper};
pub use user::{DbUser, User};

use axum::extract::Extension;
use std::sync::{Arc, OnceLock};
use tower_governor::GovernorLayer;
use tower_governor::governor::{GovernorConfig, GovernorConfigBuilder};
use utoipa_axum::router::OpenApiRouter;

use crate::AppState;
use crate::config::proto;
use crate::constants::AUTH_API_PATH;
use crate::extract::ip::RealIpKeyExtractor;

/// Signals whether the server as a GET "/" route. Useful for redirects after auth actions.
#[derive(Clone)]
pub(crate) struct HasRoot(bool);

/// Router for auth API endpoints, i.e. api/auth/v?/... .
pub(super) fn auth_router(
  config: &proto::Config,
  dev_mode: bool,
  has_root: bool,
) -> OpenApiRouter<AppState> {
  // Using the utoipa integration, we can use the on-handler metadata as the
  // source of truth for registering the routes avoiding skew.
  // Inversely, using this macro ensures that the handlers do have metadata.
  use utoipa_axum::routes;

  let mut router = OpenApiRouter::new()
    .routes(routes!(api::register::register_user_handler))
    // E-mail verification and change flows.
    .routes(routes!(
      api::verify_email::email_verification_request_handler,
    ))
    .routes(routes!(
      api::verify_email::email_verification_confirm_handler
    ))
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

  if config.auth.apple_native_client_id.is_some() {
    router = router.routes(routes!(
      api::apple_native_signin::apple_native_signin_handler
    ));
  }

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

  router = router.layer(Extension(HasRoot(has_root)));

  // Install an Ip-based rate limiter *ONLY* for auth APIs to avoid abuse.
  //
  // NOTE: If you run into rate-limits and are running behind a reverse proxy, please set the
  // "x-forwarded-for" header correctly to ensure ip-based rate limiting and request logging
  // works correctly.
  return OpenApiRouter::new().nest(&format!("/{AUTH_API_PATH}/"), {
    if let Some(rate_limit) = rate_limit(config, dev_mode) {
      router.layer(GovernorLayer::new(build_shared_governor_conf(rate_limit)))
    } else {
      router
    }
  });
}

/// Replicating minimal functionality of the above main router in case the admin dash is routed
/// from a different port to prevent cross-origin requests.
pub(super) fn admin_auth_router(config: &proto::Config, dev_mode: bool) -> OpenApiRouter<AppState> {
  // Using the utoipa integration, we can use the on-handler metadata as the
  // source of truth for registering the routes avoiding skew.
  // Inversely, using this macro ensures that the handlers do have metadata.
  use utoipa_axum::routes;

  let router = OpenApiRouter::new()
    .routes(routes!(api::login::login_handler))
    .routes(routes!(api::status::login_status_handler))
    .routes(routes!(api::logout::logout_handler))
    .layer(Extension(HasRoot(false)));

  return OpenApiRouter::new().nest(&format!("/{AUTH_API_PATH}/"), {
    if let Some(rate_limit) = rate_limit(config, dev_mode) {
      router.layer(GovernorLayer::new(build_shared_governor_conf(rate_limit)))
    } else {
      router
    }
  });
}

fn rate_limit(config: &proto::Config, dev_mode: bool) -> Option<u32> {
  if dev_mode {
    return None;
  }

  if let Some(auth_rate_limit) = config.server.auth_ip_rate_limit
    && auth_rate_limit > 0
  {
    return Some(auth_rate_limit);
  }

  return None;
}

type Governor =
  GovernorConfig<RealIpKeyExtractor, governor::middleware::StateInformationMiddleware>;

fn build_shared_governor_conf(rate_limit: u32) -> Arc<Governor> {
  static GOVERNOR_CONF: OnceLock<Arc<Governor>> = OnceLock::new();

  let governor_conf = GOVERNOR_CONF.get_or_init(|| {
    let governor_conf = Arc::new(
      GovernorConfigBuilder::default()
        // Quota.
        .burst_size(rate_limit)
        // Replenish one after 1 seconds.
        .per_second(1)
        .key_extractor(RealIpKeyExtractor)
        // Set rate limiting headers on reply.
        .use_headers()
        // Only block POST method for abuse prevention (e.g. sign-up, ...), e.g. allow unlimited
        // GET auth status.
        .methods(vec![axum::http::Method::POST])
        .finish()
        .expect("startup"),
    );

    // Periodically clean up governor.
    tokio::spawn({
      let governor_limiter = governor_conf.limiter().clone();
      async move {
        let interval = tokio::time::Duration::from_secs(60);
        loop {
          tokio::time::sleep(interval).await;
          log::trace!("rate limiting storage size: {}", governor_limiter.len());
          governor_limiter.retain_recent();
        }
      }
    });

    return governor_conf;
  });

  return governor_conf.clone();
}

#[cfg(test)]
mod auth_test;
