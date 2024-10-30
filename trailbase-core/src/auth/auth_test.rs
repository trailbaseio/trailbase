use axum::extract::{Form, Json, Path, Query, State};
use libsql::{de, params};
use std::sync::Arc;
use tower_cookies::Cookies;
use trailbase_sqlite::query_one_row;

use crate::api::TokenClaims;
use crate::app_state::{test_state, TestStateOptions};
use crate::auth::api::change_email;
use crate::auth::api::change_email::ChangeEmailConfigQuery;
use crate::auth::api::change_password::{
  change_password_handler, ChangePasswordQuery, ChangePasswordRequest,
};
use crate::auth::api::delete::delete_handler;
use crate::auth::api::login::login_with_password;
use crate::auth::api::logout::{logout_handler, LogoutQuery};
use crate::auth::api::refresh::{refresh_handler, RefreshRequest};
use crate::auth::api::register::{register_user_handler, RegisterUserRequest};
use crate::auth::api::reset_password::{
  reset_password_request_handler, reset_password_update_handler, ResetPasswordRequest,
  ResetPasswordUpdateRequest,
};
use crate::auth::api::verify_email::{verify_email_handler, VerifyEmailQuery};
use crate::auth::user::{DbUser, User};
use crate::constants::*;
use crate::email::{testing::TestAsyncSmtpTransport, Mailer};
use crate::extract::Either;

#[tokio::test]
async fn test_auth_registration_reset_and_change_email() {
  let _ = env_logger::try_init_from_env(
    env_logger::Env::new().default_filter_or("info,refinery_core=warn"),
  );

  let mailer = TestAsyncSmtpTransport::new();
  let state = test_state(Some(TestStateOptions {
    mailer: Some(Mailer::Smtp(Arc::new(mailer.clone()))),
    ..Default::default()
  }))
  .await
  .unwrap();

  let conn = state.user_conn();

  let email = "user@test.org".to_string();
  let password = "secret123".to_string();
  let session_exists_query =
    format!("SELECT EXISTS(SELECT 1 FROM '{SESSION_TABLE}' WHERE user = $1)");

  let user = {
    // Register new user and email verification flow.
    let request = RegisterUserRequest {
      email: email.clone(),
      password: password.clone(),
      password_repeat: password.clone(),
      ..Default::default()
    };

    register_user_handler(State(state.clone()), Form(request))
      .await
      .unwrap();

    // Assert that a verification email was sent.
    assert_eq!(mailer.get_logs().len(), 1);

    // Then steal the verification code from the DB and verify.
    let email_verification_code = {
      let db_user: DbUser = de::from_row(
        &query_one_row(
          conn,
          &format!("SELECT * FROM '{USER_TABLE}' WHERE email = $1"),
          [email.clone()],
        )
        .await
        .unwrap(),
      )
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

    // Check that log in pre-verification fails.
    assert!(login_with_password(&state, &email, &password)
      .await
      .is_err());

    let _ = verify_email_handler(
      State(state.clone()),
      Path(email_verification_code.clone()),
      Query(VerifyEmailQuery::default()),
    )
    .await
    .unwrap();

    let (verified, user) = {
      let db_user: DbUser = de::from_row(
        &query_one_row(
          conn,
          &format!("SELECT * FROM '{USER_TABLE}' WHERE email = $1"),
          [email.clone()],
        )
        .await
        .unwrap(),
      )
      .unwrap();

      (
        db_user.verified.clone(),
        User::from_unverified(db_user.uuid(), &db_user.email),
      )
    };

    // We should now be verified.
    assert!(verified);

    // Verifying again should fail.
    let response = verify_email_handler(
      State(state.clone()),
      Path(email_verification_code),
      Query(VerifyEmailQuery::default()),
    )
    .await;
    assert!(response.is_err());

    assert!(login_with_password(&state, &email, "Wrong Password")
      .await
      .is_err());

    let tokens = login_with_password(&state, &email, &password)
      .await
      .unwrap();
    assert_eq!(tokens.id, user.uuid);
    state
      .jwt()
      .decode::<TokenClaims>(&tokens.auth_token)
      .unwrap();

    let session_exists: bool = query_one_row(
      conn,
      &session_exists_query,
      [user.uuid.into_bytes().to_vec()],
    )
    .await
    .unwrap()
    .get(0)
    .unwrap();
    assert!(session_exists);

    user
  };

  {
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

  let reset_password = "new_password!";
  {
    // Reset (forgotten) password flow.
    reset_password_request_handler(
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
    assert!(reset_password_request_handler(
      State(state.clone()),
      Either::Json(ResetPasswordRequest {
        email: email.clone()
      }),
    )
    .await
    .is_err());

    assert_eq!(mailer.get_logs().len(), 2);

    // Steal the reset code.
    let reset_code: String = query_one_row(
      conn,
      &format!("SELECT password_reset_code FROM '{USER_TABLE}' WHERE id = $1"),
      [user.uuid.into_bytes().to_vec()],
    )
    .await
    .unwrap()
    .get(0)
    .unwrap();

    let reset_email_body: String = String::from_utf8_lossy(
      &quoted_printable::decode(
        mailer.get_logs()[1].1.as_bytes(),
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
    reset_password_update_handler(
      State(state.clone()),
      Path(reset_code.clone()),
      Either::Form(ResetPasswordUpdateRequest {
        password: new_password.clone(),
        password_repeat: new_password.clone(),
      }),
    )
    .await
    .unwrap();

    {
      assert!(login_with_password(&state, &email, &password)
        .await
        .is_err());

      let tokens = login_with_password(&state, &email, &new_password)
        .await
        .unwrap();
      assert_eq!(tokens.id, user.uuid);
      state
        .jwt()
        .decode::<TokenClaims>(&tokens.auth_token)
        .unwrap();
    }

    let _logout_response = logout_handler(
      State(state.clone()),
      Query(LogoutQuery::default()),
      Some(user.clone()),
      Cookies::default(),
    )
    .await
    .unwrap();

    let session_exists: bool = query_one_row(
      conn,
      &session_exists_query,
      [user.uuid.into_bytes().to_vec()],
    )
    .await
    .unwrap()
    .get(0)
    .unwrap();
    assert!(!session_exists);

    let tokens = login_with_password(&state, &email, &new_password)
      .await
      .unwrap();
    assert_eq!(tokens.id, user.uuid);
    state
      .jwt()
      .decode::<TokenClaims>(&tokens.auth_token)
      .unwrap();
  }

  let new_email = "new_addresses@test.org".to_string();
  {
    // Change Email flow.

    // Form requests require old email
    assert!(change_email::change_email_request_handler(
      State(state.clone()),
      user.clone(),
      Either::Form(change_email::ChangeEmailRequest {
        csrf_token: user.csrf_token.clone(),
        old_email: None,
        new_email: new_email.clone(),
      }),
    )
    .await
    .is_err());

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
    assert_eq!(mailer.get_logs().len(), 3);

    // Steal the verification code.
    let email_verification_code: String = query_one_row(
      conn,
      &format!("SELECT email_verification_code FROM '{USER_TABLE}' WHERE id = $1"),
      params!(user.uuid.into_bytes()),
    )
    .await
    .unwrap()
    .get(0)
    .unwrap();
    assert!(!email_verification_code.is_empty());

    let verification_email_body: String = String::from_utf8_lossy(
      &quoted_printable::decode(
        mailer.get_logs()[2].1.as_bytes(),
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
      Query(ChangeEmailConfigQuery { redirect_to: None }),
      user.clone(),
    )
    .await
    .expect(&format!("CODE: '{email_verification_code}'"));

    let db_email: String = query_one_row(
      conn,
      &format!("SELECT email FROM '{USER_TABLE}' WHERE id = $1"),
      params!(user.uuid.into_bytes()),
    )
    .await
    .unwrap()
    .get(0)
    .unwrap();

    assert_eq!(new_email, db_email);

    assert!(login_with_password(&state, &email, &reset_password)
      .await
      .is_err());
    let _ = login_with_password(&state, &new_email, &reset_password)
      .await
      .unwrap();
  }

  {
    // Change password flow.
    let old_password = reset_password.to_string();
    let new_password = "new_secret123".to_string();

    let _ = change_password_handler(
      State(state.clone()),
      Query(ChangePasswordQuery::default()),
      user.clone(),
      Either::Json(ChangePasswordRequest {
        old_password: old_password.clone(),
        new_password: new_password.clone(),
        new_password_repeat: new_password.clone(),
      }),
    )
    .await
    .unwrap();

    assert!(login_with_password(&state, &new_email, &password)
      .await
      .is_err());
    assert!(login_with_password(&state, &new_email, &old_password)
      .await
      .is_err());

    let _ = login_with_password(&state, &new_email, &new_password)
      .await
      .unwrap();
  }

  {
    // Delete user flow.
    delete_handler(State(state.clone()), user.clone(), Cookies::default())
      .await
      .unwrap();

    let user_exists: bool = query_one_row(
      conn,
      &format!("SELECT EXISTS(SELECT * FROM '{USER_TABLE}' WHERE id = $1)"),
      params!(user.uuid.into_bytes()),
    )
    .await
    .unwrap()
    .get(0)
    .unwrap();

    assert!(!user_exists);
  }
}
