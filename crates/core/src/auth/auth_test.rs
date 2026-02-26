use axum::extract::{Json, Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use base64::prelude::*;
use regex::Regex;
use std::sync::Arc;
use tower_cookies::Cookies;
use trailbase_sqlite::params;
use uuid::Uuid;

use crate::AppState;
use crate::api::TokenClaims;
use crate::app_state::{TestStateOptions, test_state};
use crate::auth::AuthError;
use crate::auth::api::change_email;
use crate::auth::api::change_email::ChangeEmailConfigQuery;
use crate::auth::api::change_password::{
  ChangePasswordQuery, ChangePasswordRequest, change_password_handler,
};
use crate::auth::api::delete::delete_handler;
use crate::auth::api::login::{LoginRequest, LoginResponse, login_handler};
use crate::auth::api::logout::{LogoutQuery, logout_handler};
use crate::auth::api::refresh::{RefreshRequest, refresh_handler};
use crate::auth::api::register::{RegisterUserRequest, register_user_handler};
use crate::auth::api::reset_password::{
  ResetPasswordRequest, ResetPasswordUpdateRequest, reset_password_request_handler,
  reset_password_update_handler,
};
use crate::auth::api::token::{AuthCodeToTokenRequest, TokenResponse, auth_code_to_token_handler};
use crate::auth::api::verify_email::{VerifyEmailQuery, verify_email_handler};
use crate::auth::login_params::{LoginInputParams, ResponseType};
use crate::auth::user::{DbUser, User};
use crate::auth::util::login_with_password;
use crate::constants::*;
use crate::email::{Mailer, testing::TestAsyncSmtpTransport};
use crate::extract::Either;

async fn setup_state_and_test_user(
  email: &str,
  password: &str,
) -> (AppState, TestAsyncSmtpTransport, User) {
  let _ = env_logger::try_init_from_env(
    env_logger::Env::new().default_filter_or("info,trailbase_refinery=warn"),
  );

  let mailer = TestAsyncSmtpTransport::new();
  let state = test_state(Some(TestStateOptions {
    mailer: Some(Mailer::Smtp(Arc::new(mailer.clone()))),
    ..Default::default()
  }))
  .await
  .unwrap();

  let user = register_test_user(&state, &mailer, email, password).await;
  return (state, mailer, user);
}

async fn register_test_user(
  state: &AppState,
  mailer: &TestAsyncSmtpTransport,
  email: &str,
  password: &str,
) -> User {
  // Register new user and email verification flow.
  let request = RegisterUserRequest {
    email: email.to_string(),
    password: password.to_string(),
    password_repeat: password.to_string(),
    ..Default::default()
  };

  let _ = register_user_handler(State(state.clone()), Either::Form(request))
    .await
    .unwrap();

  // Assert that a verification email was sent.
  assert_eq!(mailer.get_logs().len(), 1);

  // Then steal the verification code from the DB and verify.
  let email_verification_code = {
    let db_user = state
      .user_conn()
      .read_query_value::<DbUser>(
        format!("SELECT * FROM {USER_TABLE} WHERE email = $1"),
        params!(email.to_string()),
      )
      .await
      .unwrap()
      .unwrap();

    db_user.email_verification_code.unwrap()
  };

  let verification_email_body: String = String::from_utf8_lossy(
    &quoted_printable::decode(
      mailer.get_logs()[0].1.as_bytes(),
      quoted_printable::ParseMode::Robust,
    )
    .unwrap(),
  )
  .to_string();
  assert!(
    verification_email_body.contains(&email_verification_code),
    "code: {email_verification_code}\nbody: {verification_email_body}"
  );

  // Check that login before email verification fails.
  assert!(matches!(
    login_handler(
      State(state.clone()),
      Query(LoginInputParams::default()),
      Cookies::default(),
      Either::Json(LoginRequest {
        email: email.to_string(),
        password: password.to_string(),
        ..Default::default()
      })
    )
    .await,
    Err(AuthError::Unauthorized),
  ));

  let _ = verify_email_handler(
    State(state.clone()),
    Path(email_verification_code.clone()),
    Query(VerifyEmailQuery::default()),
  )
  .await
  .unwrap();

  let (verified, user) = {
    let db_user = state
      .user_conn()
      .read_query_value::<DbUser>(
        format!(r#"SELECT * FROM "{USER_TABLE}" WHERE email = $1"#),
        params!(email.to_string()),
      )
      .await
      .unwrap()
      .unwrap();

    (
      db_user.verified.clone(),
      User::from_unverified(db_user.uuid(), &db_user.email),
    )
  };

  // User should now be verified.
  assert!(verified);

  // Verifying again should fail.
  let response = verify_email_handler(
    State(state.clone()),
    Path(email_verification_code),
    Query(VerifyEmailQuery::default()),
  )
  .await;
  assert!(response.is_err());

  return user;
}

#[tokio::test]
async fn test_auth_password_login_flow_with_pkce() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();

  let (state, _mailer, user) = setup_state_and_test_user(&email, &password).await;

  let login_helper = async |request| {
    return login_handler(
      State(state.clone()),
      Query(LoginInputParams::default()),
      Cookies::default(),
      request,
    )
    .await;
  };

  // Test login using the PKCE flow (?response_type="code").
  let redirect_uri = "test-scheme://foo".to_string();
  let (pkce_code_challenge, pkce_code_verifier) = oauth2::PkceCodeChallenge::new_random_sha256();

  // Missing code challenge.
  assert!(matches!(
    login_helper(Either::Json(LoginRequest {
      email: email.clone(),
      password: password.clone(),
      response_type: Some(ResponseType::Code),
      redirect_uri: Some(redirect_uri.clone()),
      pkce_code_challenge: None,
      ..Default::default()
    }))
    .await,
    Err(AuthError::BadRequest(_)),
  ));

  // Missing redirect.
  assert!(matches!(
    login_helper(Either::Json(LoginRequest {
      email: email.clone(),
      password: password.clone(),
      response_type: Some(ResponseType::Code),
      redirect_uri: None,
      pkce_code_challenge: Some(pkce_code_challenge.as_str().to_string()),
      ..Default::default()
    }))
    .await,
    Err(AuthError::BadRequest(_)),
  ));

  // Bad password.
  assert!(matches!(
    &login_helper(Either::Json(LoginRequest {
      email: email.clone(),
      password: "WRONG PASSWORD".to_string(),
      response_type: Some(ResponseType::Code),
      redirect_uri: Some(redirect_uri.clone()),
      pkce_code_challenge: Some(pkce_code_challenge.as_str().to_string()),
      ..Default::default()
    }))
    .await,
    Err(AuthError::Unauthorized),
  ));

  // Finally let's log in successfully.
  let login_response = login_helper(Either::Json(LoginRequest {
    email: email.clone(),
    password: password.clone(),
    response_type: Some(ResponseType::Code),
    redirect_uri: Some(redirect_uri.clone()),
    pkce_code_challenge: Some(pkce_code_challenge.as_str().to_string()),
    ..Default::default()
  }))
  .await
  .unwrap();

  let location = url::Url::parse(&get_redirect_location(&login_response).unwrap()).unwrap();
  assert_eq!("test-scheme", location.scheme());
  let auth_code_re = Regex::new(r"^code=(.*)$").unwrap();
  let captures = auth_code_re.captures(&location.query().unwrap()).unwrap();
  let auth_code = captures.get(1).unwrap();
  assert!(!auth_code.is_empty());

  // Make sure this didn't create a session. User is not logged in before actually upgrading
  // using  the "auth code" + "code verifier".
  assert!(!session_exists(&state, user.uuid).await);

  // And now upgrade to tokens, i.e. complete log-in.
  let Json(token_response): Json<TokenResponse> = auth_code_to_token_handler(
    State(state.clone()),
    Json(AuthCodeToTokenRequest {
      authorization_code: Some(auth_code.as_str().to_string()),
      pkce_code_verifier: Some(pkce_code_verifier.secret().to_string()),
    }),
  )
  .await
  .unwrap();

  assert!(token_response.refresh_token != "");
  assert!(token_response.csrf_token != "");

  let decoded_claims = state
    .jwt()
    .decode::<TokenClaims>(&token_response.auth_token)
    .unwrap();
  assert_eq!(
    BASE64_URL_SAFE.decode(&decoded_claims.sub).unwrap(),
    user.uuid.into_bytes()
  );
  assert_eq!(decoded_claims.email, email);

  assert!(session_exists(&state, user.uuid).await);
  let _ = logout_handler(
    State(state.clone()),
    Query(LogoutQuery::default()),
    Some(user.clone()),
    Cookies::default(),
  )
  .await
  .unwrap();
  assert!(!session_exists(&state, user.uuid).await);
}

#[tokio::test]
async fn test_auth_password_login_flow_without_pkce() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();

  let (state, _mailer, user) = setup_state_and_test_user(&email, &password).await;

  let login_helper = async |request| {
    return login_handler(
      State(state.clone()),
      Query(LoginInputParams::default()),
      Cookies::default(),
      request,
    )
    .await;
  };

  // Test login using non-PKCE flow
  assert!(matches!(
    login_helper(Either::Json(LoginRequest {
      email: email.clone(),
      password: "WRONG PASSWORD".to_string(),
      ..Default::default()
    }))
    .await,
    Err(AuthError::Unauthorized),
  ));

  // Assert that form-based login yields a redirect.
  assert!(is_failed_login_redirect_response(
    &login_helper(Either::Form(LoginRequest {
      email: email.clone(),
      password: "WRONG PASSWORD".to_string(),
      ..Default::default()
    }))
    .await
    .unwrap()
  ));

  // Finally, let's try logging in with the correct password.
  let login_response: LoginResponse = {
    let response = login_helper(Either::Json(LoginRequest {
      email: email.clone(),
      password: password.clone(),
      ..Default::default()
    }))
    .await
    .unwrap();

    assert_eq!(StatusCode::OK, response.status());
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
      .await
      .unwrap();

    serde_json::from_slice(&body).unwrap()
  };

  assert!(login_response.refresh_token != "");
  assert!(login_response.csrf_token != "");

  let decoded_claims = state
    .jwt()
    .decode::<TokenClaims>(&login_response.auth_token)
    .unwrap();
  assert_eq!(
    BASE64_URL_SAFE.decode(&decoded_claims.sub).unwrap(),
    user.uuid.into_bytes()
  );
  assert_eq!(decoded_claims.email, email);

  let refresh_token: String = state
    .user_conn()
    .read_query_row_f(
      format!("SELECT refresh_token FROM {SESSION_TABLE} WHERE user = $1;"),
      (user.uuid.into_bytes().to_vec(),),
      |row| row.get(0),
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(refresh_token, login_response.refresh_token);

  assert!(session_exists(&state, user.uuid).await);
  let _ = logout_handler(
    State(state.clone()),
    Query(LogoutQuery::default()),
    Some(user.clone()),
    Cookies::default(),
  )
  .await
  .unwrap();
  assert!(!session_exists(&state, user.uuid).await);
}

#[tokio::test]
async fn test_auth_token_refresh_flow() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();

  let (state, _mailer, _user) = setup_state_and_test_user(&email, &password).await;

  // Test refresh flow.
  let tokens = login_with_password(&state, &email, &password)
    .await
    .unwrap();

  let Json(refreshed_tokens) = refresh_handler(
    State(state.clone()),
    Json(RefreshRequest {
      refresh_token: tokens.refresh_token,
    }),
  )
  .await
  .unwrap();

  let original_claims: TokenClaims = state.jwt().decode(&tokens.auth_token).unwrap();
  let refreshed_claims: TokenClaims = state.jwt().decode(&refreshed_tokens.auth_token).unwrap();

  assert_eq!(original_claims.sub, refreshed_claims.sub);
  // Make sure, they were actually re-minted.
  assert_ne!(original_claims.csrf_token, refreshed_claims.csrf_token);
  // NOTE: they're likely the same assuming they were minted most likely in the same second
  // interval.
  assert!(original_claims.iat <= refreshed_claims.iat);
  assert!(original_claims.exp <= refreshed_claims.exp);
}

#[tokio::test]
async fn test_auth_reset_password_flow() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();
  let reset_password = "new_password!";

  let (state, mailer, user) = setup_state_and_test_user(&email, &password).await;

  // Reset (forgotten) password flow.
  let _ = reset_password_request_handler(
    State(state.clone()),
    Either::Form(ResetPasswordRequest {
      email: email.clone(),
    }),
  )
  .await
  .unwrap();

  // Assert that a password reset email was sent.
  assert_eq!(mailer.get_logs().len(), 2);

  // Test rate limiting.
  assert!(
    reset_password_request_handler(
      State(state.clone()),
      Either::Json(ResetPasswordRequest {
        email: email.clone()
      }),
    )
    .await
    .is_err()
  );

  assert_eq!(mailer.get_logs().len(), 2);

  // Steal the reset code.
  let reset_code: String = state
    .user_conn()
    .read_query_row_f(
      format!("SELECT password_reset_code FROM {USER_TABLE} WHERE id = $1"),
      params!(user.uuid.into_bytes().to_vec()),
      |row| row.get(0),
    )
    .await
    .unwrap()
    .unwrap();

  let reset_email_body: String = String::from_utf8_lossy(
    &quoted_printable::decode(
      mailer.get_logs().get(1).unwrap().1.as_bytes(),
      quoted_printable::ParseMode::Robust,
    )
    .unwrap(),
  )
  .to_string();
  assert!(
    reset_email_body.contains(&reset_code),
    "code: {reset_code}\nbody: {reset_email_body}"
  );

  let new_password = reset_password.to_string();
  let _ = reset_password_update_handler(
    State(state.clone()),
    Either::Form(ResetPasswordUpdateRequest {
      password: new_password.clone(),
      password_repeat: new_password.clone(),
      password_reset_code: reset_code.clone(),
      redirect_uri: None,
    }),
  )
  .await
  .unwrap();

  {
    assert!(
      login_with_password(&state, &email, &password)
        .await
        .is_err()
    );

    let tokens = login_with_password(&state, &email, &new_password)
      .await
      .unwrap();
    assert_eq!(tokens.id, user.uuid);
    state
      .jwt()
      .decode::<TokenClaims>(&tokens.auth_token)
      .unwrap();
  }

  assert!(session_exists(&state, user.uuid).await);
  let _ = logout_handler(
    State(state.clone()),
    Query(LogoutQuery::default()),
    Some(user.clone()),
    Cookies::default(),
  )
  .await
  .unwrap();
  assert!(!session_exists(&state, user.uuid).await);

  let tokens = login_with_password(&state, &email, &new_password)
    .await
    .unwrap();
  assert_eq!(tokens.id, user.uuid);
  state
    .jwt()
    .decode::<TokenClaims>(&tokens.auth_token)
    .unwrap();
}

#[tokio::test]
async fn test_auth_change_email_flow() {
  let email = "user@test.org".to_string();
  let new_email = "new_addresses@test.org".to_string();
  let password = "secret123".to_string();

  let (state, mailer, user) = setup_state_and_test_user(&email, &password).await;

  // Form requests require old email
  assert!(
    change_email::change_email_request_handler(
      State(state.clone()),
      user.clone(),
      Either::Form(change_email::ChangeEmailRequest {
        csrf_token: user.csrf_token.clone(),
        old_email: None,
        new_email: new_email.clone(),
      }),
    )
    .await
    .is_err()
  );

  change_email::change_email_request_handler(
    State(state.clone()),
    user.clone(),
    Either::Form(change_email::ChangeEmailRequest {
      csrf_token: user.csrf_token.clone(),
      old_email: Some(email.clone()),
      new_email: new_email.clone(),
    }),
  )
  .await
  .unwrap();

  // Assert that a change-email email was sent.
  assert_eq!(mailer.get_logs().len(), 2);

  // Steal the verification code.
  let email_verification_code: String = state
    .user_conn()
    .read_query_row_f(
      format!(r#"SELECT email_verification_code FROM "{USER_TABLE}" WHERE id = $1"#),
      params!(user.uuid.into_bytes()),
      |row| row.get(0),
    )
    .await
    .unwrap()
    .unwrap();
  assert!(!email_verification_code.is_empty());

  let verification_email_body: String = String::from_utf8_lossy(
    &quoted_printable::decode(
      mailer.get_logs().get(1).unwrap().1.as_bytes(),
      quoted_printable::ParseMode::Robust,
    )
    .unwrap(),
  )
  .to_string();
  assert!(
    verification_email_body.contains(&email_verification_code),
    "code: {email_verification_code}\nbody: {verification_email_body}"
  );

  let _ = change_email::change_email_confirm_handler(
    State(state.clone()),
    Path(email_verification_code.clone()),
    Query(ChangeEmailConfigQuery { redirect_uri: None }),
    None,
  )
  .await
  .expect(&format!("CODE: '{email_verification_code}'"));

  let db_email: String = state
    .user_conn()
    .read_query_row_f(
      format!(r#"SELECT email FROM "{USER_TABLE}" WHERE id = $1"#),
      params!(user.uuid.into_bytes()),
      |row| row.get(0),
    )
    .await
    .unwrap()
    .unwrap();

  assert_eq!(new_email, db_email);

  assert!(
    login_with_password(&state, &email, &password)
      .await
      .is_err()
  );
  let _ = login_with_password(&state, &new_email, &password)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_auth_change_password_flow() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();
  let new_password = "new_secret123".to_string();

  let (state, _mailer, user) = setup_state_and_test_user(&email, &password).await;

  let _ = change_password_handler(
    State(state.clone()),
    Query(ChangePasswordQuery::default()),
    user.clone(),
    Either::Json(ChangePasswordRequest {
      old_password: password.clone(),
      new_password: new_password.clone(),
      new_password_repeat: new_password.clone(),
    }),
  )
  .await
  .unwrap();

  assert!(
    login_with_password(&state, &email, &password)
      .await
      .is_err()
  );
  assert!(
    login_with_password(&state, &email, &password)
      .await
      .is_err()
  );

  let _ = login_with_password(&state, &email, &new_password)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_auth_delete_user_flow() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();

  let (state, _mailer, user) = setup_state_and_test_user(&email, &password).await;

  let _tokens = login_with_password(&state, &email, &password)
    .await
    .unwrap();

  assert!(session_exists(&state, user.uuid).await);

  // Delete user flow.
  delete_handler(State(state.clone()), user.clone(), Cookies::default())
    .await
    .unwrap();

  let user_exists: bool = state
    .user_conn()
    .read_query_row_f(
      format!(r#"SELECT EXISTS(SELECT * FROM "{USER_TABLE}" WHERE id = $1)"#),
      params!(user.uuid.into_bytes()),
      |row| row.get(0),
    )
    .await
    .unwrap()
    .unwrap();

  assert!(!user_exists);

  assert!(!session_exists(&state, user.uuid).await);
}

async fn session_exists(state: &AppState, user_id: Uuid) -> bool {
  return state
    .user_conn()
    .read_query_row_f(
      format!("SELECT EXISTS(SELECT 1 FROM {SESSION_TABLE} WHERE user = $1)"),
      params!(user_id.into_bytes().to_vec()),
      |row| row.get(0),
    )
    .await
    .unwrap()
    .unwrap();
}

fn get_redirect_location(response: &Response) -> Option<String> {
  return response
    .headers()
    .get("location")
    .and_then(|h| h.to_str().map(|s| s.to_string()).ok());
}

fn is_failed_login_redirect_response(response: &Response) -> bool {
  return response.status() == StatusCode::SEE_OTHER
    && get_redirect_location(response).map_or(false, |location| {
      location.starts_with("/_/auth/login?alert=")
    });
}
