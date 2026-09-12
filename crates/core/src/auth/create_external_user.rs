//! Shared user-lookup/creation for OAuth flows.
//!
//! Extracted from the web-callback path so the native Sign in with Apple
//! endpoint can match and create users through the exact same code (same
//! provider-id + provider-user-id matching, same `UserIdentifier` policy).

use const_format::formatcp;
use trailbase_sqlite::{Connection, named_params, params};

use crate::auth::AuthError;
use crate::auth::oauth::OAuthUser;
use crate::auth::user::DbUser;
use crate::auth::util::validate_and_normalize_username;
use crate::config::proto;
use crate::constants::USER_TABLE;

pub(crate) async fn create_user_for_external_provider(
  conn: &Connection,
  user_identifier: proto::UserIdentifier,
  user: OAuthUser,
) -> Result<DbUser, AuthError> {
  use crate::config::proto::UserIdentifier;

  let OAuthUser {
    provider_user_id,
    provider_id,
    email,
    username,
    verified,
    avatar,
  } = user;

  let email: Option<String> = match (user_identifier, email) {
    (
      UserIdentifier::OnlyEmail
      | UserIdentifier::RequireEmail
      | UserIdentifier::RequireEmailAndUsername,
      None,
    ) => {
      return Err(AuthError::FailedDependency("missing email address".into()));
    }
    (_, None) => None,
    (UserIdentifier::OnlyUsername, Some(_)) => {
      // Drop the email even if present.
      None
    }
    (_, Some(email)) => {
      // Otherwise make sure it's verified.
      if !verified {
        return Err(AuthError::FailedDependency(
          "email address not verified".into(),
        ));
      }
      Some(email)
    }
  };

  let mut username: Option<String> = match (user_identifier, username) {
    (UserIdentifier::OnlyEmail | UserIdentifier::Undefined, _) => None,
    (
      UserIdentifier::OnlyUsername
      | UserIdentifier::RequireUsername
      | UserIdentifier::RequireEmailAndUsername,
      username,
    ) => Some(
      username
        .and_then(|u| validate_and_normalize_username(&u).ok())
        .unwrap_or_else(|| {
          // Since we strictly need a username, make one up. Users can change it later.
          format!(
            "user{suffix}",
            suffix = crate::rand::random_numeric_and_lowercase(6)
          )
        }),
    ),
    (UserIdentifier::RequireEmail, username) => username,
  };

  if let Some(username) = username.as_mut() {
    // Check availability and potentially append randomness.
    const EXISTS_QUERY: &str =
      formatcp!("SELECT EXISTS(SELECT 1 FROM \"{USER_TABLE}\" WHERE username = $1)");

    // To be pedantic we check for collisions in a loop.
    let mut i = 0;
    while conn
      .read_query_row_get::<bool>(EXISTS_QUERY, params!(username.clone()), 0)
      .await?
      .unwrap_or(false)
      && i < 5
    {
      *username = format!(
        "{username}{suffix}",
        suffix = crate::rand::random_numeric_and_lowercase(6)
      );
      i += 1;
    }

    debug_assert!(validate_and_normalize_username(username).is_ok());
  }

  const QUERY: &str = formatcp!(
    "\
      INSERT INTO \"{USER_TABLE}\" ( \
        provider_id, provider_user_id, email, username, provider_avatar_url \
      ) VALUES ( \
        :provider_id, :provider_user_id, :email, :username, :avatar \
      ) RETURNING * \
    "
  );

  let db_user: DbUser = conn
    .write_query_value(
      QUERY,
      named_params! {
          ":provider_id": provider_id as i64,
          ":provider_user_id": provider_user_id,
          ":email": email,
          ":username": username,
          ":avatar": avatar,
      },
    )
    .await?
    .ok_or_else(|| AuthError::Internal("insertion issue".into()))?;

  return Ok(db_user);
}

pub(crate) async fn user_by_provider_id(
  conn: &Connection,
  provider_id: proto::OAuthProviderId,
  provider_user_id: String,
) -> Result<Option<DbUser>, AuthError> {
  const QUERY: &str =
    formatcp!(r#"SELECT * FROM "{USER_TABLE}" WHERE provider_id = $1 AND provider_user_id = $2"#);

  return Ok(
    conn
      .read_query_value::<DbUser>(QUERY, params!(provider_id as i64, provider_user_id))
      .await?,
  );
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app_state::test_state;
  use crate::config::proto::UserIdentifier;

  #[tokio::test]
  async fn test_oauth_create_user() {
    let state = test_state(None).await.unwrap();

    fn user(username: Option<String>) -> OAuthUser {
      let rand = crate::rand::random_numeric_and_lowercase(20);
      return OAuthUser {
        provider_user_id: rand.clone(),
        provider_id: proto::OAuthProviderId::Test,
        email: Some(format!("email_{rand}@test.org")),
        username,
        verified: true,
        avatar: None,
      };
    }

    {
      let created = create_user_for_external_provider(
        state.user_conn(),
        UserIdentifier::RequireEmail,
        user(None),
      )
      .await
      .unwrap();

      assert!(created.username.is_none());
    }

    {
      let created = create_user_for_external_provider(
        state.user_conn(),
        UserIdentifier::OnlyEmail,
        user(Some("test".to_string())),
      )
      .await
      .unwrap();

      assert!(created.username.is_none());
    }

    {
      let created = create_user_for_external_provider(
        state.user_conn(),
        UserIdentifier::RequireUsername,
        user(None),
      )
      .await
      .unwrap();

      assert!(created.username.is_some());
    }

    {
      let username = "duplicate".to_string();
      let created0 = create_user_for_external_provider(
        state.user_conn(),
        UserIdentifier::RequireUsername,
        user(Some(username.clone())),
      )
      .await
      .unwrap();

      assert!(created0.username.is_some());

      let created1 = create_user_for_external_provider(
        state.user_conn(),
        UserIdentifier::RequireUsername,
        user(Some(username.clone())),
      )
      .await
      .unwrap();

      assert!(created1.username.is_some());
      assert_ne!(created0.username, created1.username);
    }
  }
}
