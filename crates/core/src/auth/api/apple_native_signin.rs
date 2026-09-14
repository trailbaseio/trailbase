use axum::extract::{Json, State};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;
use utoipa::ToSchema;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::api::login::LoginResponse;
use crate::auth::apple::{decode_and_validate_apple_id_token, fetch_apple_public_keys};
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

/// Logs users with Apple's native sign-in, i.e. a token-exchange Apple for TrailBase.
#[utoipa::path(
  post,
  path = "/apple/authorize",
  tag = "auth",
  request_body = AppleNativeLoginRequest,
  responses(
    (status = 200, description = "Converts a verified identity token to auth tokens.", body = LoginResponse),
    (status = 400, description = "Malformed token or nonce mismatch."),
    (status = 401, description = "Missing client id or token failed signature/issuer/audience/expiry verification."),
    (status = 424, description = "First-time login without a (verified) email claim under the configured user-identifier policy.")
  )
)]
pub(crate) async fn apple_native_signin_handler(
  State(state): State<AppState>,
  Json(request): Json<AppleNativeLoginRequest>,
) -> Result<Json<LoginResponse>, AuthError> {
  // Make sure native_client_id is configured, otherwise report 404 as if route had not been
  // registered.
  let native_client_id = state
    .access_config(|c| c.auth.apple_native_client_id.clone())
    .ok_or(AuthError::NotFound)?;

  // Decode and validate the token.
  let claims = {
    let public_keys = fetch_apple_public_keys(&APPLE_HTTP_CLIENT).await?;
    let claims =
      decode_and_validate_apple_id_token(&public_keys, &request.identity_token, &native_client_id)?;

    verify_nonce_claim(claims.nonce.as_deref(), &request.nonce)?;

    claims
  };

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

  return Ok(Json(LoginResponse {
    auth_token,
    refresh_token,
    csrf_token: auth_token_claims.csrf_token,
  }));
}

/// Checks the anti-replay nonce (sha256 hex-encoded) in the claims against the client's raw nonce
/// (the hash the client sent with the authorization request).
fn verify_nonce_claim(claims_nonce: Option<&str>, client_nonce: &str) -> Result<(), AuthError> {
  let Some(claims_nonce) = claims_nonce.and_then(|n| hex::decode(n).ok()) else {
    return Err(AuthError::BadRequest("identity token misses valid nonce"));
  };

  let hash = {
    let mut hasher = Sha256::new();
    hasher.update(client_nonce.as_bytes());
    hasher.finalize()
  };

  if claims_nonce == hash.as_slice() {
    return Ok(());
  }
  return Err(AuthError::BadRequest("nonce mismatch"));
}

/// Shared HTTP client for Apple's endpoints: connection pooling instead of a
/// TLS handshake per login. Redirects stay disabled like everywhere else in
/// the OAuth paths (SSRF posture), even though the JWKS URL is a constant.
///
/// Question: Should we provide a truly shared auth http client through AppState?
static APPLE_HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
  reqwest::ClientBuilder::new()
    .redirect(reqwest::redirect::Policy::none())
    .build()
    .expect("reqwest client with disabled redirects always builds")
});

#[cfg(test)]
mod tests {
  use axum::Router;
  use axum_test::TestServer;
  use tower_cookies::CookieManagerLayer;

  use super::*;
  use crate::app_state::{AppState, TestStateOptions, test_state};
  use crate::auth::apple::test_support::{APP_ID, sign_token, valid_claims};

  async fn apple_state(
    native_client_id: Option<&str>,
    user_identifier: proto::UserIdentifier,
  ) -> AppState {
    let mut config = proto::Config::new_with_custom_defaults();
    config.server.site_url = Some("https://example.org".to_string());
    config.auth.user_identifier = Some(user_identifier as i32);
    config.auth.apple_native_client_id = native_client_id.map(|id| id.to_string());
    return test_state(Some(TestStateOptions {
      config: Some(config),
      ..Default::default()
    }))
    .await
    .unwrap();
  }

  #[tokio::test]
  async fn valid_native_token_logs_in_and_creates_the_user() {
    let state = apple_state(Some(APP_ID), proto::UserIdentifier::RequireEmail).await;

    let identity_token = sign_token(valid_claims());
    let response = apple_native_signin_handler(
      State(state.clone()),
      Json(AppleNativeLoginRequest {
        identity_token,
        nonce: "test".to_string(),
      }),
    )
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
    let state = apple_state(Some(APP_ID), proto::UserIdentifier::RequireEmail).await;

    let mut claims = valid_claims();
    claims["aud"] = serde_json::json!(format!("{APP_ID}.web"));

    let result = apple_native_signin_handler(
      State(state),
      Json(AppleNativeLoginRequest {
        identity_token: sign_token(claims),
        nonce: "test".to_string(),
      }),
    )
    .await;

    assert!(matches!(result, Err(AuthError::Unauthorized)));
  }

  #[tokio::test]
  async fn nonce_mismatch_is_rejected() {
    let state = apple_state(Some(APP_ID), proto::UserIdentifier::RequireEmail).await;

    let result = apple_native_signin_handler(
      State(state),
      Json(AppleNativeLoginRequest {
        identity_token: sign_token(valid_claims()),
        nonce: "wrong-nonce".to_string(),
      }),
    )
    .await;

    assert!(matches!(
      result,
      Err(AuthError::BadRequest("nonce mismatch"))
    ));
  }

  #[tokio::test]
  async fn first_login_without_email_is_rejected() {
    let state = apple_state(Some(APP_ID), proto::UserIdentifier::RequireEmail).await;

    let mut claims = valid_claims();
    claims.as_object_mut().unwrap().remove("email");

    let result = apple_native_signin_handler(
      State(state),
      Json(AppleNativeLoginRequest {
        identity_token: sign_token(claims),
        nonce: "test".to_string(),
      }),
    )
    .await;

    assert!(matches!(result, Err(AuthError::FailedDependency(_))));
  }

  #[tokio::test]
  async fn first_login_with_unverified_email_is_rejected() {
    let state = apple_state(Some(APP_ID), proto::UserIdentifier::RequireEmail).await;

    let mut claims = valid_claims();
    claims["email_verified"] = serde_json::json!(false);

    let result = apple_native_signin_handler(
      State(state),
      Json(AppleNativeLoginRequest {
        identity_token: sign_token(claims),
        nonce: "test".to_string(),
      }),
    )
    .await;

    assert!(matches!(result, Err(AuthError::FailedDependency(_))));
  }

  #[tokio::test]
  async fn first_login_without_email_is_created_under_username_policy() {
    // Without a user-identifier policy, the shared creation path behaves
    // exactly like the web flow for a token without an email claim.
    let state = apple_state(Some(APP_ID), proto::UserIdentifier::RequireUsername).await;

    let mut claims = valid_claims();
    claims.as_object_mut().unwrap().remove("email");
    claims.as_object_mut().unwrap().remove("email_verified");

    let response = apple_native_signin_handler(
      State(state.clone()),
      Json(AppleNativeLoginRequest {
        identity_token: sign_token(claims),
        nonce: "test".to_string(),
      }),
    )
    .await
    .unwrap()
    .0;

    assert!(!response.auth_token.is_empty());
  }

  #[tokio::test]
  async fn missing_native_client_id_fails_closed_as_not_found() {
    let state = apple_state(None, proto::UserIdentifier::RequireEmail).await;

    let result = apple_native_signin_handler(
      State(state),
      Json(AppleNativeLoginRequest {
        identity_token: sign_token(valid_claims()),
        nonce: "test".to_string(),
      }),
    )
    .await;

    assert!(matches!(result, Err(AuthError::NotFound)));
  }

  /// The route is mounted and answers through the real OAuth router: a
  /// structurally invalid token is a local 400 (no network), and the route
  /// exists at all.
  #[tokio::test]
  async fn apple_native_login_route_is_mounted_and_rejects_garbage() {
    let state = apple_state(Some(APP_ID), proto::UserIdentifier::RequireEmail).await;

    let router: Router = Router::from(crate::auth::router(&state.get_config()))
      .layer(CookieManagerLayer::new())
      .with_state(state);
    let server = TestServer::new(router);

    let response = server
      .post("/apple/authorize")
      .json(&serde_json::json!({
        "identity_token": "garbage",
        "nonce": "test",
      }))
      .await;

    response.assert_status(axum::http::StatusCode::BAD_REQUEST);
  }

  #[test]
  fn nonce_claim_matches_sha256_of_test_vector() {
    let expected = hex::encode({
      let mut hasher = Sha256::new();
      hasher.update(b"test");
      hasher.finalize()
    });

    assert!(verify_nonce_claim(Some(&expected), "test").is_ok());
    assert!(verify_nonce_claim(Some("deadbeef"), "test").is_err());
    assert!(verify_nonce_claim(None, "test").is_err());
    // Self-check that the fixture claims carry the same hash.
    assert_eq!(valid_claims()["nonce"].as_str().unwrap(), expected);
  }
}
