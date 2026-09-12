//! Native Sign in with Apple login endpoint.
//!
//! The Mac App Store requires Apple sign-in to complete via the native
//! `ASAuthorizationController` sheet (App Review Guideline 4: "without leaving
//! the app"). The native flow produces an identity token directly — no
//! authorization code, no browser round-trip — so this endpoint verifies the
//! token against Apple's public keys and mints TrailBase tokens through the
//! same session path as the web OAuth callback.
//!
//! Key differences from the web flow:
//! - The token's `aud` is the App ID (`native_client_id`), not the Services ID used by the web flow
//!   — the audiences differ, hence the separate config.
//! - Replay protection is the `nonce` claim: the client sent `sha256(raw_nonce)` with the
//!   authorization request; we re-hash the raw nonce from the request body and compare.
//! - Email is only included by Apple on the FIRST authorization of the app. Repeat logins must
//!   therefore succeed without it: users are matched by Apple's team-stable `sub` and the minted
//!   auth token carries the stored email from the database.

use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;
use utoipa::ToSchema;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::apple::{decode_id_token_with_keys, extract_kid, fetch_apple_public_keys};
use crate::auth::create_external_user::{create_user_for_external_provider, user_by_provider_id};
use crate::auth::oauth::OAuthUser;
use crate::auth::tokens::{FreshTokens, mint_new_tokens};
use crate::config::proto;

#[derive(Debug, Deserialize, ToSchema)]
pub struct AppleNativeLoginRequest {
  /// The identity token from `ASAuthorizationAppleIDCredential.identityToken`.
  pub identity_token: String,
  /// The raw client-generated nonce whose SHA-256 hash was sent with the
  /// authorization request (`ASAuthorizationOpenIDRequest.nonce`).
  pub nonce: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AppleNativeTokenResponse {
  pub auth_token: String,
  pub refresh_token: String,
  pub csrf_token: String,
}

/// Logs users in with a native Sign in with Apple identity token.
#[utoipa::path(
  post,
  // QUESTION: Should this really be `/oauth/...`? What's the best practice here?
  path = "/oauth/apple/native",
  tag = "auth",
  request_body = AppleNativeLoginRequest,
  responses(
    (status = 200, description = "Converts a verified identity token to auth tokens.", body = AppleNativeTokenResponse),
    (status = 400, description = "Malformed token or nonce mismatch."),
    (status = 401, description = "Missing client id or token failed signature/issuer/audience/expiry verification."),
    (status = 424, description = "First-time login without a (verified) email claim under the configured user-identifier policy.")
  )
)]
pub(crate) async fn native_apple_login_handler(
  State(state): State<AppState>,
  Json(request): Json<AppleNativeLoginRequest>,
) -> Result<Json<AppleNativeTokenResponse>, AuthError> {
  // Fail closed when the native audience isn't configured (see
  // `OAuthProviderConfig.native_client_id`). Server-side misconfiguration,
  // not a client error.
  let native_client_id = state
    .access_config(|c| c.auth.apple_native_client_id.clone())
    .ok_or(AuthError::Unauthorized)?;

  // Structural pre-parse: rejects malformed tokens locally, before any
  // outbound request towards Apple's keys endpoint.
  let _ = extract_kid(&request.identity_token)?;

  /// Shared HTTP client for Apple's endpoints: connection pooling instead of a
  /// TLS handshake per login. Redirects stay disabled like everywhere else in
  /// the OAuth paths (SSRF posture), even though the JWKS URL is a constant.
  static APPLE_HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::ClientBuilder::new()
      .redirect(reqwest::redirect::Policy::none())
      .build()
      .expect("reqwest client with disabled redirects always builds")
  });

  let public_keys = fetch_apple_public_keys(&APPLE_HTTP_CLIENT).await?;
  // Signature, kid, issuer, audience and expiry failures all mean the
  // presented token did not authenticate.
  let claims = decode_id_token_with_keys(&public_keys, &request.identity_token, &native_client_id)
    .map_err(|_| AuthError::Unauthorized)?;

  verify_nonce_claim(claims.nonce.as_deref(), &request.nonce)?;

  // Users are matched by Apple's team-stable `sub` — the same identity space
  // as the web OAuth flow, so a web-created account and a native login resolve
  // to the same account. Creation goes through the same user-identifier
  // policy as the web flow.
  let db_user = match user_by_provider_id(
    state.user_conn(),
    proto::OAuthProviderId::Apple,
    claims.sub.clone(),
  )
  .await?
  {
    Some(existing_user) => existing_user,
    None => {
      let user_identifier = state
        .access_config(|c| c.auth.user_identifier)
        .and_then(|ui| ui.try_into().ok())
        .unwrap_or(proto::UserIdentifier::Undefined);

      create_user_for_external_provider(
        state.user_conn(),
        user_identifier,
        OAuthUser {
          provider_user_id: claims.sub,
          provider_id: proto::OAuthProviderId::Apple,
          email: claims.email,
          username: None,
          verified: claims.email_verified.is_some_and(|v| v.value()),
          avatar: None,
        },
      )
      .await?
    }
  };

  let (auth_token_ttl, refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());
  let FreshTokens {
    auth_token_claims,
    refresh_token,
    ..
  } = mint_new_tokens(
    state.session_conn(),
    &db_user,
    &auth_token_ttl,
    &refresh_token_ttl,
  )
  .await?;

  let auth_token = state
    .jwt()
    .encode(&auth_token_claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(Json(AppleNativeTokenResponse {
    auth_token,
    refresh_token,
    csrf_token: auth_token_claims.csrf_token,
  }));
}

/// Checks the anti-replay nonce: the claim must be present and equal the
/// lowercase-hex SHA-256 of the client's raw nonce (the hash the client sent
/// with the authorization request).
fn verify_nonce_claim(claims_nonce: Option<&str>, client_nonce: &str) -> Result<(), AuthError> {
  let Some(claims_nonce) = claims_nonce else {
    return Err(AuthError::BadRequest("identity token carries no nonce"));
  };

  if claims_nonce != sha256_hex(client_nonce) {
    return Err(AuthError::BadRequest("nonce mismatch"));
  }
  return Ok(());
}

fn sha256_hex(value: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(value.as_bytes());
  return hasher
    .finalize()
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect();
}

#[cfg(test)]
mod tests {
  use axum::Router;
  use axum_test::TestServer;
  use tower_cookies::CookieManagerLayer;

  use super::*;
  use crate::app_state::{AppState, TestStateOptions, test_state};
  use crate::auth::apple::test_support::{APP_ID, WEB_SERVICES_ID, sign_token, valid_claims};

  async fn apple_state(
    native_client_id: Option<&str>,
    user_identifier: Option<proto::UserIdentifier>,
  ) -> AppState {
    let mut config = proto::Config::new_with_custom_defaults();
    config.server.site_url = Some("https://example.org".to_string());
    config.auth.user_identifier = user_identifier.map(|ui| ui as i32);
    config.auth.apple_native_client_id = native_client_id.map(|id| id.to_string());
    return test_state(Some(TestStateOptions {
      config: Some(config),
      ..Default::default()
    }))
    .await
    .unwrap();
  }

  fn login_request(identity_token: &str, nonce: &str) -> AppleNativeLoginRequest {
    return AppleNativeLoginRequest {
      identity_token: identity_token.to_string(),
      nonce: nonce.to_string(),
    };
  }

  #[tokio::test]
  async fn valid_native_token_logs_in_and_creates_the_user() {
    let state = apple_state(Some(APP_ID), None).await;

    let token = sign_token(valid_claims());
    let response =
      native_apple_login_handler(State(state.clone()), Json(login_request(&token, "test")))
        .await
        .unwrap()
        .0;

    assert!(!response.auth_token.is_empty());
    assert!(!response.refresh_token.is_empty());

    // The account was created with the verified email from the token.
    let db_user = user_by_provider_id(
      state.user_conn(),
      proto::OAuthProviderId::Apple,
      "001234.abcdef.1234".to_string(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
      db_user.email.as_deref(),
      Some("user@privaterelay.appleid.com")
    );
  }

  #[tokio::test]
  async fn web_services_id_audience_is_unauthorized() {
    let state = apple_state(Some(APP_ID), None).await;

    let mut claims = valid_claims();
    claims["aud"] = serde_json::json!(WEB_SERVICES_ID);

    let result = native_apple_login_handler(
      State(state),
      Json(login_request(&sign_token(claims), "test")),
    )
    .await;

    assert!(matches!(result, Err(AuthError::Unauthorized)));
  }

  #[tokio::test]
  async fn nonce_mismatch_is_bad_request() {
    let state = apple_state(Some(APP_ID), None).await;

    let result = native_apple_login_handler(
      State(state),
      Json(login_request(&sign_token(valid_claims()), "wrong-nonce")),
    )
    .await;

    assert!(matches!(
      result,
      Err(AuthError::BadRequest("nonce mismatch"))
    ));
  }

  #[tokio::test]
  async fn first_login_without_email_is_failed_dependency_under_email_policy() {
    let state = apple_state(Some(APP_ID), Some(proto::UserIdentifier::RequireEmail)).await;

    let mut claims = valid_claims();
    claims.as_object_mut().unwrap().remove("email");

    let result = native_apple_login_handler(
      State(state),
      Json(login_request(&sign_token(claims), "test")),
    )
    .await;

    assert!(matches!(result, Err(AuthError::FailedDependency(_))));
  }

  #[tokio::test]
  async fn first_login_without_email_is_created_under_permissive_policy() {
    // Without a user-identifier policy, the shared creation path behaves
    // exactly like the web flow for a token without an email claim.
    let state = apple_state(Some(APP_ID), None).await;

    let mut claims = valid_claims();
    claims.as_object_mut().unwrap().remove("email");
    claims.as_object_mut().unwrap().remove("email_verified");

    let response = native_apple_login_handler(
      State(state.clone()),
      Json(login_request(&sign_token(claims), "test")),
    )
    .await
    .unwrap()
    .0;

    assert!(!response.auth_token.is_empty());
  }

  #[tokio::test]
  async fn missing_native_client_id_fails_closed_as_server_error() {
    let state = apple_state(None, None).await;

    let result = native_apple_login_handler(
      State(state),
      Json(login_request(&sign_token(valid_claims()), "test")),
    )
    .await;

    assert!(matches!(result, Err(AuthError::Unauthorized)));
  }

  /// The route is mounted and answers through the real OAuth router: a
  /// structurally invalid token is a local 400 (no network), and the route
  /// exists at all.
  #[tokio::test]
  async fn native_login_route_is_mounted_and_rejects_garbage_locally() {
    let state = apple_state(Some(APP_ID), None).await;

    let router: Router = Router::from(crate::auth::router(&state.get_config()))
      .layer(CookieManagerLayer::new())
      .with_state(state);
    let server = TestServer::new(router);

    let response = server
      .post("/oauth/apple/native")
      .json(&serde_json::json!({
        "identity_token": "garbage",
        "nonce": "test",
      }))
      .await;

    response.assert_status(axum::http::StatusCode::BAD_REQUEST);
  }

  /// Known-answer test pinning the cross-platform nonce contract: the claim
  /// carries the lowercase-hex SHA-256 of the raw nonce string. The same
  /// vector is referenced in the macOS (Rust) and iOS (Swift) clients.
  #[test]
  fn nonce_claim_matches_sha256_of_test_vector() {
    let expected = sha256_hex("test").to_ascii_lowercase();

    assert!(verify_nonce_claim(Some(&expected), "test").is_ok());
    assert!(verify_nonce_claim(Some("deadbeef"), "test").is_err());
    assert!(verify_nonce_claim(None, "test").is_err());
    // Self-check that the fixture claims carry the same hash.
    assert_eq!(valid_claims()["nonce"].as_str().unwrap(), expected);
  }

  #[tokio::test]
  async fn existing_apple_user_logs_in_without_email_claim() {
    let state = apple_state(Some(APP_ID), None).await;

    // First login: Apple includes the email claim only on first
    // authorization.
    let _first = native_apple_login_handler(
      State(state.clone()),
      Json(login_request(&sign_token(valid_claims()), "test")),
    )
    .await
    .unwrap();

    // Repeat login: no email claim, matched by the team-stable `sub`.
    let mut claims = valid_claims();
    claims.as_object_mut().unwrap().remove("email");
    claims.as_object_mut().unwrap().remove("email_verified");

    let response = native_apple_login_handler(
      State(state.clone()),
      Json(login_request(&sign_token(claims), "test")),
    )
    .await
    .unwrap()
    .0;

    assert!(!response.auth_token.is_empty());

    // The account still carries the email from the first authorization.
    let db_user = user_by_provider_id(
      state.user_conn(),
      proto::OAuthProviderId::Apple,
      "001234.abcdef.1234".to_string(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
      db_user.email.as_deref(),
      Some("user@privaterelay.appleid.com")
    );
  }

  #[tokio::test]
  async fn new_user_with_unverified_email_is_rejected() {
    let state = apple_state(Some(APP_ID), None).await;

    let mut claims = valid_claims();
    claims["email_verified"] = serde_json::json!(false);

    let result = native_apple_login_handler(
      State(state),
      Json(login_request(&sign_token(claims), "test")),
    )
    .await;

    assert!(matches!(result, Err(AuthError::FailedDependency(_))));
  }
}
