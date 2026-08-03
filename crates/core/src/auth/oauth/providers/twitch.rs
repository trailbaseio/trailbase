use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::auth::AuthError;
use crate::auth::oauth::OAuthUser;
use crate::auth::oauth::provider::TokenResponse;
use crate::auth::oauth::providers::social::{SocialSpec, UserApi};
use crate::config::proto::OAuthProviderId;

pub(crate) struct Twitch;

// Reference: https://dev.twitch.tv/docs/api/reference#get-users
#[derive(Default, Deserialize, Debug)]
struct TwitchUser {
  id: String,
  // According to reference above, email is implicitly verified.
  email: String,
  login: Option<String>,
  // display_name: String,
  profile_image_url: Option<String>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct TwitchUsersResponse {
  data: Vec<TwitchUser>,
}

#[async_trait]
impl SocialSpec for Twitch {
  const ID: OAuthProviderId = OAuthProviderId::Twitch;
  const NAME: &'static str = "twitch";
  const DISPLAY_NAME: &'static str = "Twitch";

  const AUTH_URL: &'static str = "https://id.twitch.tv/oauth2/authorize";
  const TOKEN_URL: &'static str = "https://id.twitch.tv/oauth2/token";
  const USER_API_URL: &'static str = "https://api.twitch.tv/helix/users";

  const SCOPES: &'static [&'static str] = &["user:read:email"];

  const AUTH_TYPE: oauth2::AuthType = oauth2::AuthType::RequestBody;

  type User = TwitchUsersResponse;

  fn user_api_headers(client_id: &str) -> Vec<(&'static str, String)> {
    return vec![("Client-Id", client_id.to_string())];
  }

  fn recover_token_response(body: &[u8]) -> Option<Result<TokenResponse, AuthError>> {
    // Twitch returns non-RFC-6749 compliant body: scopes are an array rather than space delimited
    // list.
    return Some(parse_twitch_token_response(body));
  }

  async fn map_user(
    _api: &UserApi<'_>,
    mut response: TwitchUsersResponse,
  ) -> Result<OAuthUser, AuthError> {
    let user = match response.data.len() {
      1 => response.data.swap_remove(0),
      0 => {
        return Err(AuthError::FailedDependency(
          "Twitch user response had empty data".into(),
        ));
      }
      n => {
        return Err(AuthError::FailedDependency(
          format!("Twitch user response contains {n} users").into(),
        ));
      }
    };

    return Ok(OAuthUser {
      provider_user_id: user.id,
      provider_id: Self::ID,
      email: Some(user.email),
      username: user.login,
      verified: true,
      avatar: user.profile_image_url,
    });
  }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TwitchTokenResponse {
  access_token: String,
  token_type: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  expires_in: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  refresh_token: Option<String>,
  #[serde(serialize_with = "self::serialize_space_delimited_vec")]
  scopes: Option<Vec<String>>,
}

fn parse_twitch_token_response(body: &[u8]) -> Result<TokenResponse, AuthError> {
  let token_response: TwitchTokenResponse = serde_json::from_slice(body).map_err(|_err| {
    #[cfg(debug_assertions)]
    return AuthError::FailedDependency(
      format!("Invalid twitch response: {}", String::from_utf8_lossy(body)).into(),
    );

    #[cfg(not(debug_assertions))]
    return AuthError::FailedDependency("Invalid twitch response".into());
  })?;

  return serde_json::from_value(
    serde_json::to_value(&token_response)
      .map_err(|_err| AuthError::Internal("Failed to serialize".into()))?,
  )
  .map_err(|_err| AuthError::Internal("Failed to deserialize".into()));
}

pub fn serialize_space_delimited_vec<T, S>(
  vec_opt: &Option<Vec<T>>,
  serializer: S,
) -> Result<S::Ok, S::Error>
where
  T: AsRef<str>,
  S: serde::ser::Serializer,
{
  if let Some(ref vec) = *vec_opt {
    let space_delimited = vec.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join(" ");
    serializer.serialize_str(&space_delimited)
  } else {
    serializer.serialize_none()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::auth::oauth::providers::social::resolve_user;

  #[test]
  fn parse_twitch_token_response_test() {
    let response = r#"{
      "access_token": "xxx",
      "expires_in": 13925,
      "refresh_token": "yyy",
      "scope": ["user:read:email"],
      "token_type": "bearer"
    }"#;

    parse_twitch_token_response(response.as_bytes()).unwrap();
  }

  /// Twitch wraps the user in a `data` array rather than returning it directly.
  #[tokio::test]
  async fn test_twitch_user_mapping() {
    let user = resolve_user::<Twitch>(serde_json::json!({
      "data": [{
        "id": "141981764",
        "email": "twitchdev@example.com",
        "login": "twitchdev",
        "profile_image_url": "https://static-cdn.jtvnw.net/avatar.png",
      }],
    }))
    .await
    .unwrap();

    assert_eq!(user.provider_user_id, "141981764");
    assert_eq!(user.email.as_deref(), Some("twitchdev@example.com"));
    assert_eq!(user.username.as_deref(), Some("twitchdev"));
  }

  #[tokio::test]
  async fn test_twitch_rejects_ambiguous_user_response() {
    for data in [
      serde_json::json!([]),
      serde_json::json!([
        { "id": "1", "email": "a@example.com" },
        { "id": "2", "email": "b@example.com" },
      ]),
    ] {
      let result = resolve_user::<Twitch>(serde_json::json!({ "data": data })).await;
      assert!(
        matches!(result, Err(AuthError::FailedDependency(_))),
        "{result:?}"
      );
    }
  }
}
