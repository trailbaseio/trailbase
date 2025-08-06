mod create_user;
mod delete_user;
mod list_users;
mod update_user;

pub use create_user::{CreateUserRequest, create_user_handler};
pub(super) use delete_user::delete_user_handler;
pub(super) use list_users::list_users_handler;
pub(super) use update_user::update_user_handler;

#[cfg(test)]
pub(crate) use create_user::create_user_for_test;

#[cfg(test)]
mod tests {
  use axum::{Json, extract::State};
  use std::sync::Arc;
  use trailbase_sqlite::params;
  use uuid::Uuid;

  use crate::app_state::{TestStateOptions, test_state};
  use crate::auth::util::user_by_email;
  use crate::constants::USER_TABLE;
  use crate::email::{Mailer, testing::TestAsyncSmtpTransport};

  use super::create_user::*;

  #[tokio::test]
  async fn test_user_creation_and_deletion() {
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

    let email = "foo@bar.org";
    let user_id = create_user_handler(
      State(state.clone()),
      Json(CreateUserRequest {
        email: email.to_string(),
        password: "Secret!1!!".to_string(),
        verified: true,
        admin: true,
      }),
    )
    .await
    .unwrap()
    .id;

    let user = user_by_email(&state, email).await.unwrap();
    assert_eq!(Uuid::from_bytes(user.id), user_id);

    state
      .user_conn()
      .execute(
        format!("DELETE FROM '{USER_TABLE}' WHERE id = $1"),
        params!(*user.uuid().as_bytes()),
      )
      .await
      .unwrap();

    assert!(user_by_email(&state, email).await.is_err());
  }
}
