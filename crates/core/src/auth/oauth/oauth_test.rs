use axum::extract::{Form, Json, Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{Router, get, post};
use axum_test::{TestServer, TestServerConfig};
use base64::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use tower_cookies::Cookies;
use uuid::Uuid;

use crate::api::AuthTokenClaims;
use crate::app_state::{AppState, TestStateOptions, test_state};
use crate::auth::AuthError;
use crate::auth::api::token::{
  AuthCodeToTokenRequest, TokenResponse as TokenHandlerResponse, auth_code_to_token_handler,
};
use crate::auth::login_params::{LoginInputParams, ResponseType};
use crate::auth::oauth::providers::test::{TestOAuthProvider, TestUser};
use crate::auth::oauth::state::OAuthStateClaims;
use crate::auth::oauth::{callback, list_providers, login};
use crate::auth::user::DbUser;
use crate::auth::util::derive_pkce_code_challenge;
use crate::config::proto::{Config, OAuthProviderConfig, OAuthProviderId, UserIdentifier};
use crate::constants::{
  AUTH_API_PATH, COOKIE_AUTH_TOKEN, COOKIE_OAUTH_STATE, COOKIE_REFRESH_TOKEN, SESSION_TABLE,
  USER_TABLE,
};

#[derive(Debug, Deserialize, Serialize)]
struct AuthQuery {
  response_type: String,
  client_id: String,
  state: String,
  code_challenge: String,
  code_challenge_method: String,
  redirect_uri: String,
  scope: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TokenRequest {
  grant_type: String,
  code: String,
  code_verifier: String,
  redirect_uri: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TokenResponse {
  pub access_token: String,
  pub token_type: String,
  pub request: TokenRequest,
}

const EXTERNAL_USER_ID: &str = "ExternalUserId";
const EXTERNAL_USER_EMAIL: &str = "foo@bar.com";

/// The name the OIDC factory registers itself under, i.e. the required config key.
const OIDC_PROVIDER_NAME: &str = "oidc0";

struct FakeProviderOptions {
  provider_id: OAuthProviderId,
  provider_name: String,
  /// Overrides the provider's default scopes when non-empty.
  scopes: Vec<String>,
  user_identifier: Option<UserIdentifier>,
  /// Payload served by the fake user-info endpoint.
  user_info: serde_json::Value,
}

async fn setup_fake_oauth_server(site_url: &str) -> (TestServer, AppState) {
  return setup_fake_provider(
    site_url,
    FakeProviderOptions {
      provider_id: OAuthProviderId::Test,
      provider_name: TestOAuthProvider::NAME.to_string(),
      scopes: vec![],
      user_identifier: None,
      user_info: serde_json::to_value(TestUser {
        id: EXTERNAL_USER_ID.to_string(),
        email: EXTERNAL_USER_EMAIL.to_string(),
        verified: true,
      })
      .unwrap(),
    },
  )
  .await;
}

async fn setup_fake_provider(
  site_url: &str,
  options: FakeProviderOptions,
) -> (TestServer, AppState) {
  const AUTH_PATH: &str = "/auth";
  const TOKEN_PATH: &str = "/token";
  const USER_INFO_PATH: &str = "/user";

  let FakeProviderOptions {
    provider_id,
    provider_name,
    scopes,
    user_identifier,
    user_info,
  } = options;

  let app = Router::new()
    // AUTH endpoint takes: app info, desired auth flow (e.g. PKCE) and provides a redirect to the
    // provider's login form. Called by TB's /oauth/<provider>/login handler.
    .route(
      AUTH_PATH,
      get(|Query(query): Query<AuthQuery>| async {
        // Silly pipe-through. Just makes it easier to test the query arguments.
        return Json(query);
      }),
    )
    // TOKEN endpoint converts auth_code + PKCE_code_verifier into tokens. Called by TB's callback
    // handler to get external tokens and call the USER_INFO endpoint below.
    .route(
      TOKEN_PATH,
      post(|Form(req): Form<TokenRequest>| async move {
        Json(TokenResponse {
          access_token: "opaque_token".to_string(),
          token_type: "Bearer".to_string(),
          request: req,
        })
      }),
    )
    // USER_INFO endpoint provides user information given an autorized get request, e.g. tokens in
    // the cookies. Called by TB's /oauth/<provider>/callback.
    .route(USER_INFO_PATH, get(|| async move { Json(user_info) }));

  let server = TestServer::new_with_config(
    app,
    TestServerConfig {
      transport: Some(axum_test::Transport::HttpRandomPort),
      ..Default::default()
    },
  );

  let state = test_state(Some(TestStateOptions {
    config: Some({
      let mut config = Config::new_with_custom_defaults();
      config.server.site_url = Some(site_url.to_string());
      config.auth.user_identifier = user_identifier.map(|ui| ui as i32);
      config.auth.oauth_providers = [(
        provider_name.clone(),
        OAuthProviderConfig {
          client_id: Some("test_client_id".to_string()),
          client_secret: Some("test_client_secret".to_string()),
          provider_id: Some(provider_id as i32),
          // OIDC paths
          auth_url: Some(server.server_url(AUTH_PATH).unwrap().to_string()),
          token_url: Some(server.server_url(TOKEN_PATH).unwrap().to_string()),
          user_api_url: Some(server.server_url(USER_INFO_PATH).unwrap().to_string()),
          scopes,
          ..Default::default()
        },
      )]
      .into();
      config
    }),
    ..Default::default()
  }))
  .await
  .unwrap();

  // List OAuth providers and make sure our fake OIDC provider is in there.
  let auth_options = state.auth_options();
  let providers = auth_options.list_oauth_providers();
  assert_eq!(providers.len(), 1);
  assert_eq!(providers[0].name, provider_name);

  let Json(response) = list_providers::list_configured_providers_handler(State(state.clone()))
    .await
    .unwrap();
  assert_eq!(response.providers.len(), 1);
  assert_eq!(response.providers[0].0, provider_name);

  return (server, state);
}

#[tokio::test]
async fn test_oauth_login_flow_without_pkce() {
  let site_url = "https://bar.org";
  let (_server, state) = setup_fake_oauth_server(site_url).await;

  // Call TB's OAuth login handler, which will produce a redirect for users to get the external
  // auth provider's login form.
  let cookies = Cookies::default();
  let redirect_uri = format!("{site_url}/login-success-welcome");
  let external_redirect: Redirect = login::login_with_external_auth_provider(
    State(state.clone()),
    Path(TestOAuthProvider::NAME.to_string()),
    Query(LoginInputParams {
      redirect_uri: Some(redirect_uri.to_string()),
      mfa_redirect_uri: None,
      response_type: None,
      pkce_code_challenge: None,
    }),
    cookies.clone(),
  )
  .await
  .unwrap();

  // Extract ephemeral OAoauth cookie state set by TB in login handler.
  let oauth_state: OAuthStateClaims = state
    .jwt()
    .decode(cookies.get(COOKIE_OAUTH_STATE).unwrap().value())
    .unwrap();

  // Call the fake server's auth endpoint.
  let redirect_uri_external_login = get_redirect_location(external_redirect).unwrap();
  // NOTE: The dummy implementation just pipes the input query params through. We could do the
  // following assertions equally on `redirect_uri_external_login`
  let redirect_uri_external_login_url = url::Url::parse(&redirect_uri_external_login).unwrap();
  let query_params: HashMap<Cow<'_, str>, Cow<'_, str>> =
    redirect_uri_external_login_url.query_pairs().collect();

  let auth_query: AuthQuery = reqwest::get(&redirect_uri_external_login)
    .await
    .unwrap()
    .json()
    .await
    .unwrap();

  assert_eq!(query_params.get("client_id").unwrap(), "test_client_id");
  assert_eq!(auth_query.client_id, "test_client_id");
  // NOTE: the response type is between TB and the external provider. Not between the user and TB.
  assert_eq!(auth_query.response_type, "code");
  assert_eq!(auth_query.state, oauth_state.csrf_secret);
  assert_eq!(
    auth_query.redirect_uri,
    format!(
      "{site_url}/{AUTH_API_PATH}/oauth/{}/callback",
      TestOAuthProvider::NAME
    )
  );
  assert_eq!(
    auth_query.code_challenge,
    derive_pkce_code_challenge(&oauth_state.pkce_code_verifier)
  );

  // Pretend to be the browser and call TB's OAuth callback handler.
  let internal_redirect = callback::callback_from_external_auth_provider(
    State(state.clone()),
    Path(TestOAuthProvider::NAME.to_string()),
    Query(callback::AuthQuery {
      state: auth_query.state.clone(),
      code: auth_query.code_challenge.clone(),
    }),
    cookies.clone(),
  )
  .await
  .unwrap();

  let location = get_redirect_location(internal_redirect).unwrap();
  assert_eq!(location, redirect_uri);

  // Check user exists.
  let db_user = state
    .user_conn()
    .read_query_value::<DbUser>(
      format!("SELECT * FROM {USER_TABLE} WHERE provider_user_id = $1"),
      (EXTERNAL_USER_ID,),
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(EXTERNAL_USER_EMAIL, db_user.email.as_deref().unwrap());

  // Is logged in.
  assert!(session_exists(&state, db_user.uuid()).await);

  // And we have tokens.
  let auth_token = cookies.get(COOKIE_AUTH_TOKEN).unwrap().value().to_string();
  let decoded_claims = state.jwt().decode::<AuthTokenClaims>(&auth_token).unwrap();
  assert_eq!(db_user.email, decoded_claims.email);
  let refresh_token = cookies
    .get(COOKIE_REFRESH_TOKEN)
    .unwrap()
    .value()
    .to_string();
  assert!(!refresh_token.is_empty(), "{refresh_token}");
}

#[tokio::test]
async fn test_oauth_login_flow_with_pkce() {
  let site_url = "https://bar.org";
  let (_server, state) = setup_fake_oauth_server(site_url).await;

  // Call TB's OAuth login handler, which will produce a redirect for users to get the external
  // auth provider's login form.
  let cookies = Cookies::default();
  let redirect_uri = format!("{site_url}/login-success-welcome");
  let (pkce_code_challenge, pkce_code_verifier) = oauth2::PkceCodeChallenge::new_random_sha256();
  let external_redirect: Redirect = login::login_with_external_auth_provider(
    State(state.clone()),
    Path(TestOAuthProvider::NAME.to_string()),
    Query(LoginInputParams {
      redirect_uri: Some(redirect_uri.to_string()),
      mfa_redirect_uri: None,
      response_type: Some(ResponseType::Code),
      pkce_code_challenge: Some(pkce_code_challenge.as_str().to_string()),
    }),
    cookies.clone(),
  )
  .await
  .unwrap();

  // Extract ephemeral OAoauth cookie state set by TB in login handler.
  let oauth_state: OAuthStateClaims = state
    .jwt()
    .decode(cookies.get(COOKIE_OAUTH_STATE).unwrap().value())
    .unwrap();

  // Call the fake server's auth endpoint.
  let redirect_uri_external_login = get_redirect_location(external_redirect).unwrap();
  // NOTE: The dummy implementation just pipes the input query params through. We could do the
  // following assertions equally on `redirect_uri_external_login`
  let redirect_uri_external_login_url = url::Url::parse(&redirect_uri_external_login).unwrap();
  let query_params: HashMap<Cow<'_, str>, Cow<'_, str>> =
    redirect_uri_external_login_url.query_pairs().collect();

  let auth_query: AuthQuery = reqwest::get(&redirect_uri_external_login)
    .await
    .unwrap()
    .json()
    .await
    .unwrap();

  assert_eq!(query_params.get("client_id").unwrap(), "test_client_id");
  assert_eq!(auth_query.client_id, "test_client_id");
  // NOTE: the response type is between TB and the external provider. Not between the user and TB.
  assert_eq!(auth_query.response_type, "code");
  assert_eq!(auth_query.state, oauth_state.csrf_secret);
  assert_eq!(
    auth_query.redirect_uri,
    format!(
      "{site_url}/{AUTH_API_PATH}/oauth/{}/callback",
      TestOAuthProvider::NAME
    )
  );
  assert_eq!(
    auth_query.code_challenge,
    derive_pkce_code_challenge(&oauth_state.pkce_code_verifier)
  );

  // Pretend to be the browser and call TB's OAuth callback handler.
  let internal_redirect = callback::callback_from_external_auth_provider(
    State(state.clone()),
    Path(TestOAuthProvider::NAME.to_string()),
    Query(callback::AuthQuery {
      state: auth_query.state.clone(),
      code: auth_query.code_challenge.clone(),
    }),
    cookies.clone(),
  )
  .await
  .unwrap();

  let location_str = get_redirect_location(internal_redirect).unwrap();
  let location = url::Url::parse(&location_str).unwrap();
  assert!(location_str.starts_with(&format!("{redirect_uri}?code=")));

  let auth_code_re = Regex::new(r"^code=(.*)$").unwrap();
  let captures = auth_code_re.captures(&location.query().unwrap()).unwrap();
  let auth_code = captures.get(1).unwrap();
  assert!(!auth_code.is_empty());

  // Check user exists.
  let db_user = state
    .user_conn()
    .read_query_value::<DbUser>(
      format!("SELECT * FROM {USER_TABLE} WHERE provider_user_id = $1"),
      (EXTERNAL_USER_ID,),
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(EXTERNAL_USER_EMAIL, db_user.email.as_deref().unwrap());

  // And session does not yet exist before upgrading auth_code + verifier to tokens.
  assert!(!session_exists(&state, db_user.uuid()).await);

  // Upgrade to tokens, i.e. complete log-in.
  let Json(token_response): Json<TokenHandlerResponse> = auth_code_to_token_handler(
    State(state.clone()),
    Json(AuthCodeToTokenRequest {
      authorization_code: Some(auth_code.as_str().to_string()),
      pkce_code_verifier: Some(pkce_code_verifier.secret().to_string()),
    }),
  )
  .await
  .unwrap();

  // And check session exists.
  assert!(session_exists(&state, db_user.uuid()).await);

  assert!(token_response.refresh_token != "");
  assert!(token_response.csrf_token != "");

  let decoded_claims = state
    .jwt()
    .decode::<AuthTokenClaims>(&token_response.auth_token)
    .unwrap();
  assert_eq!(
    BASE64_URL_SAFE.decode(&decoded_claims.sub).unwrap(),
    db_user.uuid().into_bytes()
  );
  assert_eq!(
    EXTERNAL_USER_EMAIL,
    decoded_claims.email.as_deref().unwrap()
  );
}

#[tokio::test]
async fn test_oidc_requests_default_scopes() {
  let site_url = "https://bar.org";
  let (_server, state) = setup_fake_provider(
    site_url,
    FakeProviderOptions {
      provider_id: OAuthProviderId::Oidc0,
      provider_name: OIDC_PROVIDER_NAME.to_string(),
      scopes: vec![],
      user_identifier: None,
      user_info: serde_json::json!({
        "sub": EXTERNAL_USER_ID,
        "email": EXTERNAL_USER_EMAIL,
        "email_verified": true,
        "preferred_username": "external_user",
      }),
    },
  )
  .await;

  let (auth_query, result) = run_login_flow(&state, OIDC_PROVIDER_NAME, site_url).await;
  result.unwrap();

  assert_eq!(auth_query.scope, "openid email profile");

  let db_user = user_by_provider_user_id(&state, EXTERNAL_USER_ID)
    .await
    .unwrap();
  assert_eq!(EXTERNAL_USER_EMAIL, db_user.email.as_deref().unwrap());
}

/// Providers that only grant `openid` are the motivation for configurable scopes: without an
/// `email` scope there's no email claim, so the user has to be identified by username instead.
#[tokio::test]
async fn test_oidc_with_configured_scopes_and_no_email_claim() {
  let site_url = "https://bar.org";
  let (_server, state) = setup_fake_provider(
    site_url,
    FakeProviderOptions {
      provider_id: OAuthProviderId::Oidc0,
      provider_name: OIDC_PROVIDER_NAME.to_string(),
      scopes: vec!["openid".to_string()],
      user_identifier: Some(UserIdentifier::RequireUsername),
      user_info: serde_json::json!({ "sub": EXTERNAL_USER_ID }),
    },
  )
  .await;

  let (auth_query, result) = run_login_flow(&state, OIDC_PROVIDER_NAME, site_url).await;
  result.unwrap();

  assert_eq!(auth_query.scope, "openid");

  let db_user = user_by_provider_user_id(&state, EXTERNAL_USER_ID)
    .await
    .unwrap();
  assert_eq!(db_user.email, None);
  // No `preferred_username` claim either, so one gets made up.
  assert!(db_user.username.is_some());
  assert!(session_exists(&state, db_user.uuid()).await);
}

/// The flip-side of the above: an email-based `UserIdentifier` cannot represent an emailless user,
/// so login must fail rather than create an account that no flow can address.
#[tokio::test]
async fn test_oidc_no_email_claim_is_rejected_for_email_identifier() {
  let site_url = "https://bar.org";
  let (_server, state) = setup_fake_provider(
    site_url,
    FakeProviderOptions {
      provider_id: OAuthProviderId::Oidc0,
      provider_name: OIDC_PROVIDER_NAME.to_string(),
      scopes: vec!["openid".to_string()],
      user_identifier: Some(UserIdentifier::RequireEmail),
      user_info: serde_json::json!({ "sub": EXTERNAL_USER_ID }),
    },
  )
  .await;

  let (_auth_query, result) = run_login_flow(&state, OIDC_PROVIDER_NAME, site_url).await;
  assert!(
    matches!(result, Err(AuthError::BadRequest(_))),
    "{:?}",
    result.err()
  );

  assert!(
    user_by_provider_user_id(&state, EXTERNAL_USER_ID)
      .await
      .is_none()
  );
}

/// Drives the cookie-based login flow end-to-end, returning what the provider's auth endpoint saw
/// alongside the outcome of TrailBase's callback handler.
async fn run_login_flow(
  state: &AppState,
  provider_name: &str,
  site_url: &str,
) -> (AuthQuery, Result<Response, AuthError>) {
  let cookies = Cookies::default();
  let external_redirect = login::login_with_external_auth_provider(
    State(state.clone()),
    Path(provider_name.to_string()),
    Query(LoginInputParams {
      redirect_uri: Some(format!("{site_url}/login-success-welcome")),
      mfa_redirect_uri: None,
      response_type: None,
      pkce_code_challenge: None,
    }),
    cookies.clone(),
  )
  .await
  .unwrap();

  let auth_query: AuthQuery = reqwest::get(&get_redirect_location(external_redirect).unwrap())
    .await
    .unwrap()
    .json()
    .await
    .unwrap();

  let result = callback::callback_from_external_auth_provider(
    State(state.clone()),
    Path(provider_name.to_string()),
    Query(callback::AuthQuery {
      state: auth_query.state.clone(),
      code: auth_query.code_challenge.clone(),
    }),
    cookies,
  )
  .await;

  return (auth_query, result);
}

async fn user_by_provider_user_id(state: &AppState, provider_user_id: &str) -> Option<DbUser> {
  return state
    .user_conn()
    .read_query_value::<DbUser>(
      format!("SELECT * FROM {USER_TABLE} WHERE provider_user_id = $1"),
      (provider_user_id.to_string(),),
    )
    .await
    .unwrap();
}

fn get_redirect_location<T: IntoResponse>(response: T) -> Option<String> {
  return response
    .into_response()
    .headers()
    .get("location")
    .and_then(|h| h.to_str().map(|s| s.to_string()).ok());
}

async fn session_exists(state: &AppState, user_id: Uuid) -> bool {
  return state
    .session_conn()
    .read_query_row_get(
      format!("SELECT EXISTS(SELECT 1 FROM {SESSION_TABLE} WHERE user = $1)"),
      (user_id.into_bytes().to_vec(),),
      0,
    )
    .await
    .unwrap()
    .unwrap();
}
