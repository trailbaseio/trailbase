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
use crate::api::AuthTokenClaims;
use crate::app_state::{TestStateOptions, test_state};
use crate::auth::AuthError;
use crate::auth::api::change_email::{self, ChangeEmailConfigParams};
use crate::auth::api::change_password::{
  ChangePasswordParams, ChangePasswordRequest, change_password_handler,
};
use crate::auth::api::change_username::{
  ChangeUsernameParams, ChangeUsernameRequest, change_username_handler,
};
use crate::auth::api::delete::delete_handler;
use crate::auth::api::login::{
  LoginMfaRequest, LoginRequest, LoginResponse, MfaTokenResponse, login_handler, login_mfa_handler,
};
use crate::auth::api::login_anonymous::{
  LoginAnonymousRequest, cleanup_anonymous_users, login_anonymous_user_handler,
};
use crate::auth::api::logout::{LogoutParams, logout_handler};
use crate::auth::api::otp;
use crate::auth::api::promote_anonymous::{
  PromoteAnonymousRequest, promote_anonymous_user_handler,
};
use crate::auth::api::refresh::{RefreshRequest, refresh_handler};
use crate::auth::api::register::{RegisterUserParams, RegisterUserRequest, register_user_handler};
use crate::auth::api::reset_password::{
  ResetPasswordRequest, ResetPasswordUpdateRequest, reset_password_request_handler,
  reset_password_update_handler,
};
use crate::auth::api::token::{AuthCodeToTokenRequest, TokenResponse, auth_code_to_token_handler};
use crate::auth::api::totp;
use crate::auth::api::verify_email::{VerifyEmailParams, verify_email_handler};
use crate::auth::jwt::PasswordResetTokenClaims;
use crate::auth::login_params::{LoginInputParams, ResponseType};
use crate::auth::user::{DbUser, User};
use crate::auth::util::login_with_password;
use crate::config::proto::{Config, EmailTemplate, UserIdentifier};
use crate::constants::*;
use crate::email::{Mailer, testing::TestAsyncSmtpTransport};
use crate::extract::Either;

fn build_test_config_with_trivial_tokens() -> Config {
  let mut config = crate::app_state::test_config();
  config.email.password_reset_template = Some(EmailTemplate {
    subject: None,
    body: Some("{{ TOKEN }}".to_string()),
  });
  config.email.change_email_template = Some(EmailTemplate {
    subject: None,
    body: Some("{{ TOKEN }}".to_string()),
  });
  config.email.user_verification_template = Some(EmailTemplate {
    subject: None,
    body: Some("{{ TOKEN }}".to_string()),
  });
  return config;
}

async fn setup_state_and_test_user(
  email: &str,
  password: &str,
  config: Option<Config>,
) -> (AppState, TestAsyncSmtpTransport, User) {
  let _ = env_logger::try_init_from_env(
    env_logger::Env::new().default_filter_or("info,trailbase_refinery=warn"),
  );

  let mailer = TestAsyncSmtpTransport::new();

  let state = test_state(Some(TestStateOptions {
    mailer: Some(Mailer::Smtp(Arc::new(mailer.clone()))),
    config: Some({
      let mut config = config.unwrap_or_else(build_test_config_with_trivial_tokens);

      config.auth.enable_otp_signin = Some(true);

      config
    }),
    ..Default::default()
  }))
  .await
  .unwrap();

  let user = register_test_user(
    &state,
    &mailer,
    Identifier::Email(email.to_string()),
    password,
  )
  .await
  .unwrap();

  return (state, mailer, user);
}

enum Identifier {
  Email(String),
  Username(String),
  EmailAndUsername(String, String),
}

async fn register_test_user(
  state: &AppState,
  mailer: &TestAsyncSmtpTransport,
  identifier: Identifier,
  password: &str,
) -> Result<User, anyhow::Error> {
  // Register new user and email verification flow.

  let request = match identifier {
    Identifier::Email(ref email) => RegisterUserRequest {
      email: Some(email.clone()),
      username: None,
      password: password.to_string(),
      password_repeat: password.to_string(),
      ..Default::default()
    },
    Identifier::EmailAndUsername(ref email, ref username) => RegisterUserRequest {
      email: Some(email.clone()),
      username: Some(username.clone()),
      password: password.to_string(),
      password_repeat: password.to_string(),
      ..Default::default()
    },
    Identifier::Username(ref username) => RegisterUserRequest {
      email: None,
      username: Some(username.clone()),
      password: password.to_string(),
      password_repeat: password.to_string(),
      ..Default::default()
    },
  };

  let has_email = request.email.is_some();

  let _ = register_user_handler(
    State(state.clone()),
    Query(RegisterUserParams::default()),
    Either::Form(request),
  )
  .await?;

  // Assert that a verification email was sent.
  if has_email {
    assert_eq!(mailer.get_logs().len(), 1);

    // Then steal the verification code from the DB and verify.
    let verification_email_body: String = String::from_utf8_lossy(&quoted_printable::decode(
      mailer.get_logs()[0].1.as_bytes(),
      quoted_printable::ParseMode::Robust,
    )?)
    .to_string();

    let verification_email_re = Regex::new(r"\n(ey.*)$").unwrap();
    let verification_email_token: String = verification_email_re
      .captures(&verification_email_body)
      .unwrap()
      .get(1)
      .unwrap()
      .as_str()
      .to_string();

    // Check that login before email verification fails.
    assert!(matches!(
      login_handler(
        State(state.clone()),
        Query(LoginInputParams::default()),
        Cookies::default(),
        Either::Json(match identifier {
          Identifier::Email(ref email) | Identifier::EmailAndUsername(ref email, _) =>
            LoginRequest::Email {
              email: email.clone(),
              password: password.to_string(),
              params: LoginInputParams {
                ..Default::default()
              },
            },
          Identifier::Username(ref username) => LoginRequest::Username {
            username: username.clone(),
            password: password.to_string(),
            params: LoginInputParams {
              ..Default::default()
            },
          },
        })
      )
      .await,
      Err(AuthError::Unauthorized),
    ));

    let _ = verify_email_handler(
      State(state.clone()),
      Path(verification_email_token.clone()),
      Query(VerifyEmailParams::default()),
    )
    .await?;
  }

  let db_user = match identifier {
    Identifier::Email(email) | Identifier::EmailAndUsername(email, _) => {
      let db_user = state
        .user_conn()
        .read_query_value::<DbUser>(
          format!(r#"SELECT * FROM "{USER_TABLE}" WHERE email = $1"#),
          params!(email.clone()),
        )
        .await?
        .unwrap();

      // User should now be verified.
      assert!(db_user.verified);

      db_user
    }
    Identifier::Username(username) => state
      .user_conn()
      .read_query_value::<DbUser>(
        format!(r#"SELECT * FROM "{USER_TABLE}" WHERE username = $1"#),
        params!(username.to_string()),
      )
      .await?
      .unwrap(),
  };

  return Ok(User::from_unverified(
    db_user.uuid(),
    db_user.email.as_deref(),
    db_user.username.as_deref(),
  ));
}

#[tokio::test]
async fn test_auth_password_login_flow_with_pkce() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();

  let (state, _mailer, user) = setup_state_and_test_user(&email, &password, None).await;

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
    login_helper(Either::Json(LoginRequest::Email {
      email: email.clone(),
      password: password.clone(),
      params: LoginInputParams {
        response_type: Some(ResponseType::Code),
        redirect_uri: Some(redirect_uri.clone()),
        pkce_code_challenge: None,
        ..Default::default()
      },
    }))
    .await,
    Err(AuthError::BadRequest(_)),
  ));

  // Missing redirect.
  assert!(matches!(
    login_helper(Either::Json(LoginRequest::Email {
      email: email.clone(),
      password: password.clone(),
      params: LoginInputParams {
        response_type: Some(ResponseType::Code),
        redirect_uri: None,
        pkce_code_challenge: Some(pkce_code_challenge.as_str().to_string()),
        ..Default::default()
      }
    }))
    .await,
    Err(AuthError::BadRequest(_)),
  ));

  // Bad password.
  assert!(matches!(
    &login_helper(Either::Json(LoginRequest::Email {
      email: email.clone(),
      password: "WRONG PASSWORD".to_string(),
      params: LoginInputParams {
        response_type: Some(ResponseType::Code),
        redirect_uri: Some(redirect_uri.clone()),
        pkce_code_challenge: Some(pkce_code_challenge.as_str().to_string()),
        ..Default::default()
      }
    }))
    .await,
    Err(AuthError::Unauthorized),
  ));

  // Finally let's log in successfully.
  login_helper(Either::Json(LoginRequest::Email {
    // Make sure capitalization doesn't matter.
    email: "usER@test.org".to_string(),
    password: password.clone(),
    params: LoginInputParams {
      response_type: Some(ResponseType::Code),
      redirect_uri: Some(redirect_uri.clone()),
      pkce_code_challenge: Some(pkce_code_challenge.as_str().to_string()),
      ..Default::default()
    },
  }))
  .await
  .unwrap();

  let login_response = login_helper(Either::Json(LoginRequest::Email {
    email: email.clone(),
    password: password.clone(),
    params: LoginInputParams {
      response_type: Some(ResponseType::Code),
      redirect_uri: Some(redirect_uri.clone()),
      pkce_code_challenge: Some(pkce_code_challenge.as_str().to_string()),
      ..Default::default()
    },
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
    .decode::<AuthTokenClaims>(&token_response.auth_token)
    .unwrap();
  assert_eq!(
    BASE64_URL_SAFE.decode(&decoded_claims.sub).unwrap(),
    user.uuid.into_bytes()
  );
  assert_eq!(decoded_claims.email, Some(email));

  assert!(session_exists(&state, user.uuid).await);
  let _ = logout_handler(
    State(state.clone()),
    Query(LogoutParams::default()),
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

  let (state, _mailer, user) = setup_state_and_test_user(&email, &password, None).await;

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
    login_helper(Either::Json(LoginRequest::Email {
      email: email.clone(),
      password: "WRONG PASSWORD".to_string(),
      params: LoginInputParams {
        ..Default::default()
      }
    }))
    .await,
    Err(AuthError::Unauthorized),
  ));

  {
    // Assert that form-based login yields a redirect.
    let response = login_helper(Either::Form(LoginRequest::Email {
      email: email.clone(),
      password: "WRONG PASSWORD".to_string(),
      params: LoginInputParams {
        redirect_uri: Some("/_/auth/login".to_string()),
        ..Default::default()
      },
    }))
    .await
    .unwrap();

    assert!(
      response.status() == StatusCode::SEE_OTHER
        && get_redirect_location(&response)
          .unwrap()
          .starts_with("/_/auth/login?alert=")
    )
  }

  // Finally, let's try logging in with the correct password.
  let login_response: LoginResponse = {
    let response = login_helper(Either::Json(LoginRequest::Email {
      email: email.clone(),
      password: password.clone(),
      params: LoginInputParams {
        ..Default::default()
      },
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
    .decode::<AuthTokenClaims>(&login_response.auth_token)
    .unwrap();
  assert_eq!(
    BASE64_URL_SAFE.decode(&decoded_claims.sub).unwrap(),
    user.uuid.into_bytes()
  );
  assert_eq!(decoded_claims.email, Some(email));

  let refresh_token: String = state
    .session_conn()
    .read_query_row_get(
      format!("SELECT refresh_token FROM {SESSION_TABLE} WHERE user = $1;"),
      (user.uuid.into_bytes().to_vec(),),
      0,
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(refresh_token, login_response.refresh_token);

  assert!(session_exists(&state, user.uuid).await);
  let _ = logout_handler(
    State(state.clone()),
    Query(LogoutParams::default()),
    Some(user.clone()),
    Cookies::default(),
  )
  .await
  .unwrap();
  assert!(!session_exists(&state, user.uuid).await);
}

#[tokio::test]
async fn test_auth_password_login_flow_with_totp() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();

  let (state, _mailer, user) = setup_state_and_test_user(&email, &password, None).await;

  let login_helper = async |request| {
    return login_handler(
      State(state.clone()),
      Query(LoginInputParams::default()),
      Cookies::default(),
      request,
    )
    .await;
  };

  let response = totp::register_totp_request_handler(
    State(state.clone()),
    Query(Default::default()),
    user.clone(),
  )
  .await
  .unwrap();
  assert_eq!(200, response.status());

  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();

  let totp::RegisterTotpResponse { totp_url, .. } = serde_json::from_slice(&body).unwrap();

  let t = totp_rs::TOTP::from_url(&totp_url).unwrap();

  let response = totp::register_totp_confirm_handler(
    State(state.clone()),
    user.clone(),
    Either::Json(totp::ConfirmRegisterTotpRequest {
      totp_url: totp_url.clone(),
      totp: t.generate_current().unwrap(),
    }),
  )
  .await
  .unwrap();

  assert_eq!(200, response.status());

  let response = login_helper(Either::Json(LoginRequest::Email {
    email: email.clone(),
    password: password.clone(),
    params: LoginInputParams {
      ..Default::default()
    },
  }))
  .await
  .unwrap();

  // Make sure it's a MFA response.
  assert_eq!(403, response.status());

  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();

  let MfaTokenResponse { mfa_token, .. } = serde_json::from_slice(&body).unwrap();

  let response = login_mfa_handler(
    State(state.clone()),
    Query(Default::default()),
    Cookies::default(),
    Either::Json(LoginMfaRequest {
      mfa_token,
      totp: Some(t.generate_current().unwrap()),
      params: Default::default(),
    }),
  )
  .await
  .unwrap();

  assert_eq!(200, response.status());
}

#[tokio::test]
async fn test_auth_token_refresh_flow() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();

  let (state, _mailer, _user) = setup_state_and_test_user(&email, &password, None).await;

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

  let original_claims: AuthTokenClaims = state.jwt().decode(&tokens.auth_token).unwrap();
  let refreshed_claims: AuthTokenClaims = state.jwt().decode(&refreshed_tokens.auth_token).unwrap();

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

  let (state, mailer, user) = setup_state_and_test_user(&email, &password, None).await;

  // Reset (forgotten) password flow.
  let _ = reset_password_request_handler(
    State(state.clone()),
    Query(Default::default()),
    Either::Form(ResetPasswordRequest::Email {
      email: email.clone(),
      params: Default::default(),
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
      Query(Default::default()),
      Either::Json(ResetPasswordRequest::Email {
        email: email.clone(),
        params: Default::default(),
      }),
    )
    .await
    .is_err()
  );

  assert_eq!(mailer.get_logs().len(), 2);

  // Steal the reset code.
  let reset_email_body: String = String::from_utf8_lossy(
    &quoted_printable::decode(
      mailer.get_logs().get(1).unwrap().1.as_bytes(),
      quoted_printable::ParseMode::Robust,
    )
    .unwrap(),
  )
  .to_string();

  let password_reset_re = Regex::new(r"\n(ey.*)$").unwrap();
  let password_reset_token: String = password_reset_re
    .captures(&reset_email_body)
    .unwrap()
    .get(1)
    .unwrap()
    .as_str()
    .to_string();

  assert!(
    PasswordResetTokenClaims::from_password_reset_token(state.jwt(), &password_reset_token).is_ok(),
    "{password_reset_token}"
  );

  let new_password = reset_password.to_string();
  let _ = reset_password_update_handler(
    State(state.clone()),
    Query(Default::default()),
    Either::Form(ResetPasswordUpdateRequest {
      password: new_password.clone(),
      password_repeat: new_password.clone(),
      password_reset_token: password_reset_token,
      ..Default::default()
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
      .decode::<AuthTokenClaims>(&tokens.auth_token)
      .unwrap();
  }

  assert!(session_exists(&state, user.uuid).await);
  let _ = logout_handler(
    State(state.clone()),
    Query(LogoutParams::default()),
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
    .decode::<AuthTokenClaims>(&tokens.auth_token)
    .unwrap();
}

#[tokio::test]
async fn test_auth_change_email_flow() {
  let email = "user@test.org".to_string();
  let new_email = "new_addresses@test.org".to_string();
  let password = "secret123".to_string();

  let (state, mailer, user) = setup_state_and_test_user(&email, &password, None).await;

  // Form requests require old email
  assert!(
    change_email::change_email_request_handler(
      State(state.clone()),
      user.clone(),
      Query(Default::default()),
      Either::Form(change_email::ChangeEmailRequest {
        csrf_token: user.csrf_token.clone(),
        old_email: None,
        new_email: Some(new_email.clone()),
        ..Default::default()
      }),
    )
    .await
    .is_err()
  );

  change_email::change_email_request_handler(
    State(state.clone()),
    user.clone(),
    Query(Default::default()),
    Either::Form(change_email::ChangeEmailRequest {
      csrf_token: user.csrf_token.clone(),
      old_email: Some(email.clone()),
      new_email: Some(new_email.clone()),
      ..Default::default()
    }),
  )
  .await
  .unwrap();

  // Assert that a change-email email was sent.
  assert_eq!(mailer.get_logs().len(), 2);

  // Steal the change email verification code.
  let change_email_body: String = String::from_utf8_lossy(
    &quoted_printable::decode(
      mailer.get_logs().get(1).unwrap().1.as_bytes(),
      quoted_printable::ParseMode::Robust,
    )
    .unwrap(),
  )
  .to_string();

  let change_email_re = Regex::new(r"\n(ey.*)$").unwrap();
  let change_email_token: String = change_email_re
    .captures(&change_email_body)
    .unwrap()
    .get(1)
    .unwrap()
    .as_str()
    .to_string();

  let _ = change_email::change_email_confirm_handler(
    State(state.clone()),
    Path(change_email_token.clone()),
    Query(ChangeEmailConfigParams { redirect_uri: None }),
  )
  .await
  .expect(&format!("CODE: '{change_email_token}'"));

  let db_email: String = state
    .user_conn()
    .read_query_row_get(
      format!(r#"SELECT email FROM "{USER_TABLE}" WHERE id = $1"#),
      params!(user.uuid.into_bytes()),
      0,
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

  let (state, _mailer, user) = setup_state_and_test_user(&email, &password, None).await;

  let _ = change_password_handler(
    State(state.clone()),
    Query(ChangePasswordParams::default()),
    user.clone(),
    Either::Json(ChangePasswordRequest {
      old_password: password.clone(),
      new_password: new_password.clone(),
      new_password_repeat: Some(new_password.clone()),
      ..Default::default()
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
async fn test_auth_change_username_flow() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();

  let (state, _mailer, user) = setup_state_and_test_user(
    &email,
    &password,
    Some({
      let mut config = build_test_config_with_trivial_tokens();
      config.auth.user_identifier = Some(UserIdentifier::RequireEmail.into());
      config
    }),
  )
  .await;

  assert!(
    change_username_handler(
      State(state.clone()),
      Query(ChangeUsernameParams::default()),
      user.clone(),
      Either::Json(ChangeUsernameRequest {
        new_username: Some("!invalid_handle".to_string()),
        params: ChangeUsernameParams::default(),
      }),
    )
    .await
    .is_err()
  );

  let _ = change_username_handler(
    State(state.clone()),
    Query(ChangeUsernameParams::default()),
    user.clone(),
    Either::Json(ChangeUsernameRequest {
      new_username: Some("Foo".to_string()),
      params: ChangeUsernameParams::default(),
    }),
  )
  .await
  .unwrap();

  let _ = change_username_handler(
    State(state.clone()),
    Query(ChangeUsernameParams::default()),
    user.clone(),
    Either::Json(ChangeUsernameRequest {
      new_username: None,
      params: ChangeUsernameParams::default(),
    }),
  )
  .await
  .unwrap();
}

#[tokio::test]
async fn test_auth_register_handle_only() {
  let username = "foo".to_string();
  let password = "secret123".to_string();

  let mailer = TestAsyncSmtpTransport::new();
  let state = test_state(Some(TestStateOptions {
    mailer: Some(Mailer::Smtp(Arc::new(mailer.clone()))),
    config: Some({
      let mut config = build_test_config_with_trivial_tokens();
      config.auth.user_identifier = Some(UserIdentifier::OnlyUsername.into());
      config
    }),
    ..Default::default()
  }))
  .await
  .unwrap();

  assert!(
    register_test_user(
      &state,
      &mailer,
      Identifier::EmailAndUsername("user@test.org".to_string(), username.clone()),
      &password,
    )
    .await
    .is_err()
  );

  let user = register_test_user(
    &state,
    &mailer,
    Identifier::Username(username.clone()),
    &password,
  )
  .await
  .unwrap();

  cleanup_anonymous_users(state.user_conn()).await.unwrap();

  assert!(user.email.is_none());
  assert_eq!(Some(&username), user.username.as_ref());

  login_handler(
    State(state.clone()),
    Query(LoginInputParams::default()),
    Cookies::default(),
    Either::Json(LoginRequest::Username {
      username: username.clone(),
      password: password.to_string(),
      params: LoginInputParams {
        ..Default::default()
      },
    }),
  )
  .await
  .unwrap();
}

#[tokio::test]
async fn test_auth_change_username_and_unset_email_flow() {
  let email = "user@test.org".to_string();
  let username = "foo".to_string();
  let password = "secret123".to_string();

  let mailer = TestAsyncSmtpTransport::new();
  let state = test_state(Some(TestStateOptions {
    mailer: Some(Mailer::Smtp(Arc::new(mailer.clone()))),
    config: Some({
      let mut config = build_test_config_with_trivial_tokens();
      config.auth.user_identifier = Some(UserIdentifier::RequireUsername.into());
      config
    }),
    ..Default::default()
  }))
  .await
  .unwrap();

  assert!(
    register_test_user(&state, &mailer, Identifier::Email(email.clone()), &password)
      .await
      .is_err()
  );

  let user = register_test_user(
    &state,
    &mailer,
    Identifier::EmailAndUsername(email, username),
    &password,
  )
  .await
  .unwrap();

  println!("USER: {user:?}");

  let rows = state
    .user_conn()
    .read_query_rows("SELECT * FROM _user;", ())
    .await
    .unwrap();
  println!("ROWS: {rows:?}");

  change_email::change_email_request_handler(
    State(state.clone()),
    user.clone(),
    Query(Default::default()),
    Either::Json(change_email::ChangeEmailRequest {
      csrf_token: user.csrf_token.clone(),
      new_email: None,
      ..Default::default()
    }),
  )
  .await
  .unwrap();
}

#[tokio::test]
async fn test_auth_delete_user_flow() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();

  let (state, _mailer, user) = setup_state_and_test_user(&email, &password, None).await;

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
    .read_query_row_get(
      format!(r#"SELECT EXISTS(SELECT * FROM "{USER_TABLE}" WHERE id = $1)"#),
      params!(user.uuid.into_bytes()),
      0,
    )
    .await
    .unwrap()
    .unwrap();

  assert!(!user_exists);

  assert!(!session_exists(&state, user.uuid).await);
}

#[tokio::test]
async fn test_auth_otp_flow_using_email() {
  let email = "user@test.org".to_string();
  let password = "secret123".to_string();

  let (state, mailer, user) = setup_state_and_test_user(&email, &password, None).await;

  assert_eq!(Some(&email), user.email.as_ref());

  // NOTE: We return a success response on unknown user to avoid leaks.
  otp::request_otp_handler(
    State(state.clone()),
    Query(Default::default()),
    Either::Form(otp::RequestOtpRequest::Email {
      email: "unknown@user.org".to_string(),
      params: Default::default(),
    }),
  )
  .await
  .unwrap();

  // Only verify-email email for "user@test.org"
  assert_eq!(mailer.get_logs().len(), 1, "{:?}", mailer.get_logs());

  otp::request_otp_handler(
    State(state.clone()),
    Query(Default::default()),
    Either::Form(otp::RequestOtpRequest::Email {
      email: user.email.as_ref().unwrap().clone(),
      params: Default::default(),
    }),
  )
  .await
  .unwrap();

  // Assert that a verification email was sent.
  assert_eq!(mailer.get_logs().len(), 2);

  // Then steal the verification code from the DB and verify.
  let otp_email_body: String = String::from_utf8_lossy(
    &quoted_printable::decode(
      mailer.get_logs()[1].1.as_bytes(),
      quoted_printable::ParseMode::Robust,
    )
    .unwrap(),
  )
  .to_string();

  assert!(
    otp::login_otp_handler(
      State(state.clone()),
      Cookies::default(),
      Query(Default::default()),
      Either::Form(otp::LoginOtpRequest {
        params: otp::LoginOtpParams {
          email: user.email.clone(),
          code: Some("InvalidCode".to_string()),
          ..Default::default()
        }
      }),
    )
    .await
    .is_err()
  );

  let otp_email_re = Regex::new(r"code=([a-zA-Z0-9]*)").unwrap();
  let otp_email_code: String = otp_email_re
    .captures(&otp_email_body)
    .expect(&format!("{otp_email_body}"))
    .get(1)
    .unwrap()
    .as_str()
    .to_string();

  assert_eq!(otp_email_code.len(), 6, "Got: '{otp_email_code}'");

  let response = otp::login_otp_handler(
    State(state.clone()),
    Cookies::default(),
    Query(Default::default()),
    Either::Json(otp::LoginOtpRequest {
      params: otp::LoginOtpParams {
        // Make sure trimming/normalization works.
        // email: Some(format!("{email}  ")),
        email: Some("useR@test.org".to_string()),
        code: Some(format!("{otp_email_code} ")),
        ..Default::default()
      },
    }),
  )
  .await
  .unwrap();

  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();

  let _login_response: LoginResponse = serde_json::from_slice(&body).unwrap();
}

#[tokio::test]
async fn test_auth_otp_flow_using_username() {
  let email = "user@test.org".to_string();
  let username = "foo".to_string();
  let password = "secret123".to_string();

  let mailer = TestAsyncSmtpTransport::new();
  let state = test_state(Some(TestStateOptions {
    mailer: Some(Mailer::Smtp(Arc::new(mailer.clone()))),
    config: Some({
      let mut config = build_test_config_with_trivial_tokens();
      config.auth.user_identifier = Some(UserIdentifier::RequireUsername.into());
      config.auth.enable_otp_signin = Some(true);
      config
    }),
    ..Default::default()
  }))
  .await
  .unwrap();

  let user = register_test_user(
    &state,
    &mailer,
    Identifier::EmailAndUsername(email, username.clone()),
    &password,
  )
  .await
  .unwrap();

  assert_eq!(Some(&username), user.username.as_ref());

  // NOTE: We return a success response on unknown user to avoid leaks.
  otp::request_otp_handler(
    State(state.clone()),
    Query(Default::default()),
    Either::Form(otp::RequestOtpRequest::Username {
      username: "unknown".to_string(),
      params: Default::default(),
    }),
  )
  .await
  .unwrap();

  // Only verify-email email for "user@test.org"
  assert_eq!(mailer.get_logs().len(), 1, "{:?}", mailer.get_logs());

  otp::request_otp_handler(
    State(state.clone()),
    Query(Default::default()),
    Either::Form(otp::RequestOtpRequest::Username {
      username: username.clone(),
      params: Default::default(),
    }),
  )
  .await
  .unwrap();

  // Assert that a verification email was sent.
  assert_eq!(mailer.get_logs().len(), 2);

  // Then steal the verification code from the DB and verify.
  let otp_email_body: String = String::from_utf8_lossy(
    &quoted_printable::decode(
      mailer.get_logs()[1].1.as_bytes(),
      quoted_printable::ParseMode::Robust,
    )
    .unwrap(),
  )
  .to_string();

  assert!(
    otp::login_otp_handler(
      State(state.clone()),
      Cookies::default(),
      Query(Default::default()),
      Either::Form(otp::LoginOtpRequest {
        params: otp::LoginOtpParams {
          username: Some(username.clone()),
          code: Some("InvalidCode".to_string()),
          ..Default::default()
        }
      }),
    )
    .await
    .is_err()
  );

  let otp_email_re = Regex::new(r"code=([a-zA-Z0-9]*)").unwrap();
  let otp_email_code: String = otp_email_re
    .captures(&otp_email_body)
    .expect(&format!("{otp_email_body}"))
    .get(1)
    .unwrap()
    .as_str()
    .to_string();

  assert_eq!(otp_email_code.len(), 6, "Got: '{otp_email_code}'");

  let response = otp::login_otp_handler(
    State(state.clone()),
    Cookies::default(),
    Query(Default::default()),
    Either::Json(otp::LoginOtpRequest {
      params: otp::LoginOtpParams {
        // Make sure trimming/normalization works.
        username: Some(format!(" {username} ")),
        code: Some(format!(" {otp_email_code} ")),
        ..Default::default()
      },
    }),
  )
  .await
  .unwrap();

  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();

  let _login_response: LoginResponse = serde_json::from_slice(&body).unwrap();
}

#[tokio::test]
async fn test_auth_annonymous_signin() {
  // let email = "user@test.org".to_string();
  // let username = "foo".to_string();

  let mailer = TestAsyncSmtpTransport::new();
  let state = test_state(Some(TestStateOptions {
    mailer: Some(Mailer::Smtp(Arc::new(mailer.clone()))),
    config: Some({
      let mut config = build_test_config_with_trivial_tokens();
      config.auth.user_identifier = Some(UserIdentifier::RequireUsername.into());
      config.auth.enable_anonymous_signin = Some(true);
      config
    }),
    ..Default::default()
  }))
  .await
  .unwrap();

  let response = login_anonymous_user_handler(
    State(state.clone()),
    Query(Default::default()),
    Cookies::default(),
    Either::Json(LoginAnonymousRequest {
      params: Default::default(),
    }),
  )
  .await
  .unwrap();

  assert_eq!(StatusCode::OK, response.status());
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();

  let login_response: LoginResponse = serde_json::from_slice(&body).unwrap();

  let user = User::from_auth_token(&state, &login_response.auth_token).unwrap();
  let Some(ref username) = user.username else {
    panic!("missing username");
  };
  assert!(username.starts_with("anon"));

  let password = "secret123".to_string();
  let new_username = "user".to_string();

  assert_eq!(mailer.get_logs().len(), 0);

  promote_anonymous_user_handler(
    State(state.clone()),
    Query(Default::default()),
    user.clone(),
    Either::Json(PromoteAnonymousRequest {
      new_password: password.clone(),
      new_password_repeat: Some(password.clone()),
      new_username: Some(new_username.clone()),
      new_email: Some("user@test.org".to_string()),
      params: Default::default(),
    }),
  )
  .await
  .unwrap();

  // Assert that a verification email was sent.
  assert_eq!(mailer.get_logs().len(), 1);

  assert!(
    login_handler(
      State(state.clone()),
      Query(LoginInputParams::default()),
      Cookies::default(),
      Either::Json(LoginRequest::Username {
        username: new_username.clone(),
        password: password.clone(),
        params: LoginInputParams {
          ..Default::default()
        },
      }),
    )
    .await
    .is_err()
  );

  state
    .user_conn()
    .execute_batch(format!(
      "UPDATE {USER_TABLE} SET verified = 1 WHERE username = \"user\";"
    ))
    .await
    .unwrap();

  login_handler(
    State(state.clone()),
    Query(LoginInputParams::default()),
    Cookies::default(),
    Either::Json(LoginRequest::Username {
      username: new_username.clone(),
      password: password.clone(),
      params: LoginInputParams {
        ..Default::default()
      },
    }),
  )
  .await
  .unwrap();
}

async fn session_exists(state: &AppState, user_id: Uuid) -> bool {
  return state
    .session_conn()
    .read_query_row_get(
      format!("SELECT EXISTS(SELECT 1 FROM {SESSION_TABLE} WHERE user = $1)"),
      params!(user_id.into_bytes().to_vec()),
      0,
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
