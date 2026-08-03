use async_trait::async_trait;
use serde::Deserialize;

use crate::auth::AuthError;
use crate::auth::oauth::providers::client::UserApi;
use crate::auth::oauth::providers::social::{ExternalUser, SocialSpec};
use crate::config::proto::OAuthProviderId;

pub(crate) struct Discord;

//  Checkout available fields on: https://discord.com/developers/docs/resources/user
#[derive(Default, Deserialize, Debug)]
pub(crate) struct DiscordUser {
  id: String,
  email: String,
  verified: bool,

  // discriminator: Option<String>,
  username: Option<String>,
  avatar: Option<String>,
}

#[async_trait]
impl SocialSpec for Discord {
  const ID: OAuthProviderId = OAuthProviderId::Discord;
  const NAME: &'static str = "discord";
  const DISPLAY_NAME: &'static str = "Discord";

  const AUTH_URL: &'static str = "https://discord.com/oauth2/authorize";
  const TOKEN_URL: &'static str = "https://discord.com/api/oauth2/token";
  const USER_API_URL: &'static str = "https://discord.com/api/users/@me";

  const SCOPES: &'static [&'static str] = &["identify", "email"];

  type User = DiscordUser;

  async fn map_user(_api: &UserApi<'_>, user: DiscordUser) -> Result<ExternalUser, AuthError> {
    // let username = match (user.discriminator, user.username) {
    //   (Some(discriminator), Some(username)) => Some(format!("{username}#{discriminator}")),
    //   (None, Some(username)) => Some(username.to_string()),
    //   (Some(discriminator), None) => Some(discriminator.to_string()),
    //   (None, None) => None,
    // };

    return Ok(ExternalUser {
      // Discord only hands out the avatar's hash, the CDN URL is ours to build.
      avatar: user.avatar.map(|avatar| {
        format!(
          "https://cdn.discordapp.com/avatars/{id}/{avatar}.png",
          id = user.id
        )
      }),
      provider_user_id: user.id,
      email: Some(user.email),
      username: user.username,
      verified: user.verified,
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::auth::oauth::providers::social::resolve_user;

  #[tokio::test]
  async fn test_discord_user_mapping() {
    let user = resolve_user::<Discord>(serde_json::json!({
      "id": "80351110224678912",
      "email": "user@example.com",
      "verified": true,
      "username": "nelly",
      "avatar": "8342729096ea3675442027381ff50dfe",
    }))
    .await
    .unwrap();

    assert_eq!(user.email.as_deref(), Some("user@example.com"));
    assert_eq!(user.username.as_deref(), Some("nelly"));
    // Discord only hands out the avatar's hash, the CDN URL is ours to build.
    assert_eq!(
      user.avatar.as_deref(),
      Some(
        "https://cdn.discordapp.com/avatars/80351110224678912/8342729096ea3675442027381ff50dfe.png"
      )
    );
  }

  #[tokio::test]
  async fn test_discord_rejects_unverified_user() {
    let result = resolve_user::<Discord>(serde_json::json!({
      "id": "80351110224678912",
      "email": "user@example.com",
      "verified": false,
    }))
    .await;

    assert!(matches!(result, Err(AuthError::Unauthorized)), "{result:?}");
  }
}
