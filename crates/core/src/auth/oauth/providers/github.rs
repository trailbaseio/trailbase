use async_trait::async_trait;
use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::providers::client::UserApi;
use crate::auth::oauth::providers::social::{ExternalUser, SocialSpec};
use crate::config::proto::OAuthProviderId;

pub(crate) struct Github;

//  Checkout available fields on: https://docs.github.com/en/rest/users/users?apiVersion=2026-03-10
#[derive(Default, Deserialize, Debug)]
pub(crate) struct GithubUser {
  id: i64,
  login: Option<String>,
  // name: String,
  email: Option<String>,
  // verified: bool,
  avatar_url: Option<String>,
}

#[derive(Default, Deserialize, Debug)]
struct GithubEmail {
  email: String,
  primary: bool,
  verified: bool,
  // NOTE: null | "private" | "public"
  // visibility: Option<String>,
}

#[async_trait]
impl SocialSpec for Github {
  const ID: OAuthProviderId = OAuthProviderId::Github;
  const NAME: &'static str = "github";
  const DISPLAY_NAME: &'static str = "GitHub";

  const AUTH_URL: &'static str = "https://github.com/login/oauth/authorize";
  const TOKEN_URL: &'static str = "https://github.com/login/oauth/access_token";
  // const DEVICE_AUTH_URL: &'static str = "https://github.com/login/device/code";
  const USER_API_URL: &'static str = "https://api.github.com/user";

  const SCOPES: &'static [&'static str] = &["read:user", "user:email"];

  type User = GithubUser;

  fn user_api_headers(_client_id: &str) -> Vec<(&'static str, String)> {
    // Github rejects requests without a user agent.
    return vec![("User-Agent", "TrailBase".to_string())];
  }

  async fn map_user(api: &UserApi<'_>, user: GithubUser) -> Result<ExternalUser, AuthError> {
    // Users can set the "Keep my email private" option, in which case the user api will return an
    // empty email and we'll have to call the dedicated `/emails` endpoint.
    let email = if let Some(email) = user.email
      && !email.is_empty()
    {
      email
    } else {
      let emails: Vec<GithubEmail> = api
        .get_json(&format!("{}/emails", api.user_api_url()))
        .await?;

      let Some(primary) = emails
        .into_iter()
        .find(|cand| cand.verified && cand.primary)
      else {
        return Err(AuthError::FailedDependency("missing email".into()));
      };

      primary.email
    };

    return Ok(ExternalUser {
      provider_user_id: user.id.to_string(),
      email: Some(email),
      username: user.login,
      verified: true,
      avatar: user.avatar_url,
    });
  }
}

#[cfg(test)]
mod tests {
  use axum::Json;
  use axum::routing::{Router, get};

  use super::*;
  use crate::auth::oauth::providers::social::{
    USER_API_TEST_PATH, resolve_user, resolve_user_against,
  };

  #[tokio::test]
  async fn test_github_user_mapping() {
    let user = resolve_user::<Github>(serde_json::json!({
      "id": 1234,
      "login": "octocat",
      "email": "octocat@github.com",
      "avatar_url": "https://github.com/images/octocat.gif",
    }))
    .await
    .unwrap();

    // Github ids are numeric, ours are strings.
    assert_eq!(user.provider_user_id, "1234");
    assert_eq!(user.email.as_deref(), Some("octocat@github.com"));
    assert_eq!(user.username.as_deref(), Some("octocat"));
  }

  /// Users with "Keep my email private" force a second call to `/emails`, out of which only the
  /// verified primary address may be used.
  #[tokio::test]
  async fn test_github_falls_back_to_email_endpoint() {
    for private_email in [serde_json::Value::Null, "".into()] {
      let user = resolve_user_against::<Github>(github_routes(
        serde_json::json!({
          "id": 1234,
          "login": "octocat",
          "email": private_email,
        }),
        serde_json::json!([
          { "email": "unverified@github.com", "primary": true, "verified": false },
          { "email": "secondary@github.com", "primary": false, "verified": true },
          { "email": "primary@github.com", "primary": true, "verified": true },
        ]),
      ))
      .await
      .unwrap();

      assert_eq!(user.email.as_deref(), Some("primary@github.com"));
    }
  }

  #[tokio::test]
  async fn test_github_without_any_usable_email() {
    let result = resolve_user_against::<Github>(github_routes(
      serde_json::json!({ "id": 1234, "login": "octocat" }),
      serde_json::json!([{ "email": "a@github.com", "primary": true, "verified": false }]),
    ))
    .await;

    assert!(
      matches!(result, Err(AuthError::FailedDependency(_))),
      "{result:?}"
    );
  }

  fn github_routes(user: serde_json::Value, emails: serde_json::Value) -> Router {
    return Router::new()
      .route(USER_API_TEST_PATH, get(|| async move { Json(user) }))
      .route(
        &format!("{USER_API_TEST_PATH}/emails"),
        get(|| async move { Json(emails) }),
      );
  }
}
