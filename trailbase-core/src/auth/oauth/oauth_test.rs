use axum::extract::{Form, Json, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post, Router};
use axum_test::{TestServer, TestServerConfig};
use serde::{Deserialize, Serialize};
use tower_cookies::Cookies;

use crate::app_state::{test_state, TestStateOptions};
use crate::auth::oauth::providers::test::{TestOAuthProvider, TestUser};
use crate::auth::oauth::state::OAuthState;
use crate::auth::oauth::{callback, list_providers, login};
use crate::auth::util::derive_pkce_code_challenge;
use crate::config::proto::{Config, OAuthProviderConfig, OAuthProviderId};
use crate::constants::{AUTH_API_PATH, COOKIE_OAUTH_STATE, USER_TABLE};

fn unpack_redirect(redirect: Redirect) -> String {
  let response = redirect.into_response();
  let headers = response.headers();
  return headers
    .get("location")
    .unwrap()
    .to_str()
    .unwrap()
    .to_string();
}

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

#[tokio::test]
async fn test_oauth() {
  let name = TestOAuthProvider::NAME.to_string();
  let external_user_id = "ExternalUserId";
  let external_user_email = "foo@bar.com";

  let auth_path = "/auth";
  let token_path = "/token";
  let user_api_path = "/user";
  let app = Router::new()
    .route(
      auth_path,
      get(|Query(query): Query<AuthQuery>| async { Json(query) }),
    )
    .route(
      token_path,
      post(|Form(req): Form<TokenRequest>| async move {
        Json(TokenResponse {
          access_token: "opaque_token".to_string(),
          token_type: "Bearer".to_string(),
          request: req,
        })
      }),
    )
    .route(
      user_api_path,
      get(|| async {
        Json(TestUser {
          id: external_user_id.to_string(),
          email: external_user_email.to_string(),
          verified: true,
        })
      }),
    );

  let server = TestServer::new_with_config(
    app,
    TestServerConfig {
      transport: Some(axum_test::Transport::HttpRandomPort),
      ..Default::default()
    },
  )
  .unwrap();

  let mut config = Config::new_with_custom_defaults();
  config.auth.oauth_providers.insert(
    name.clone(),
    OAuthProviderConfig {
      client_id: Some("test_client_id".to_string()),
      client_secret: Some("test_client_secret".to_string()),
      provider_id: Some(OAuthProviderId::Custom as i32),
      // TODO: Set it up to talk to a fake/mock server.
      auth_url: Some(server.server_url(auth_path).unwrap().to_string()),
      token_url: Some(server.server_url(token_path).unwrap().to_string()),
      user_api_url: Some(server.server_url(user_api_path).unwrap().to_string()),
      ..Default::default()
    },
  );

  let state = test_state(Some(TestStateOptions {
    config: Some(config),
    ..Default::default()
  }))
  .await
  .unwrap();

  let providers = state.get_oauth_providers();
  assert_eq!(providers.len(), 1);
  assert_eq!(providers[0].0, TestOAuthProvider::NAME);

  let Json(response) = list_providers::list_configured_providers_handler(State(state.clone()))
    .await
    .unwrap();
  assert_eq!(response.providers.len(), 1);
  assert_eq!(response.providers[0].0, TestOAuthProvider::NAME);

  let cookies = Cookies::default();
  // Redirect to auth provider for the user to log in on their site.
  let external_redirect: Redirect = login::login_with_external_auth_provider(
    State(state.clone()),
    Path(name.clone()),
    Query(login::LoginQuery {
      redirect_to: None,
      response_type: None,
      pkce_code_challenge: None,
    }),
    cookies.clone(),
  )
  .await
  .unwrap();

  let response = reqwest::get(&unpack_redirect(external_redirect))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let auth_query: AuthQuery = response.json().await.unwrap();

  assert_eq!(auth_query.response_type, "code");
  assert_eq!(auth_query.client_id, "test_client_id");

  let oauth_state: OAuthState = state
    .jwt()
    .decode(cookies.get(COOKIE_OAUTH_STATE).unwrap().value())
    .unwrap();

  assert_eq!(auth_query.state, oauth_state.csrf_secret);
  assert_eq!(
    auth_query.redirect_uri,
    format!("http://localhost:4000/{AUTH_API_PATH}/oauth/{name}/callback")
  );
  assert_eq!(
    auth_query.code_challenge,
    derive_pkce_code_challenge(&oauth_state.pkce_code_verifier)
  );

  // Pretend to be the browser and call the callback handler.
  let internal_redirect = callback::callback_from_external_auth_provider(
    State(state.clone()),
    Path(name.clone()),
    Query(callback::AuthRequest {
      state: auth_query.state.clone(),
      code: auth_query.code_challenge.clone(),
    }),
    cookies.clone(),
  )
  .await
  .unwrap();

  let location = unpack_redirect(internal_redirect);
  assert_eq!(location, "/_/auth/profile");

  let row = state
    .user_conn()
    .query_row(
      &format!(r#"SELECT email FROM "{USER_TABLE}" WHERE provider_user_id = $1"#),
      (external_user_id,),
    )
    .await
    .unwrap()
    .unwrap();

  assert_eq!(row.get::<String>(0).unwrap(), external_user_email);
}
