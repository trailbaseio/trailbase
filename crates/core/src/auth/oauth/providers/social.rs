use async_trait::async_trait;
use oauth2::AuthType;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::marker::PhantomData;

use crate::auth::AuthError;
use crate::auth::oauth::providers::client::{ProviderClient, UserApi};
use crate::auth::oauth::providers::interface::TokenResponse;
use crate::auth::oauth::providers::{OAuthProviderError, OAuthProviderFactory};
use crate::auth::oauth::{OAuthClientSettings, OAuthProvider, OAuthUser};
use crate::config::proto::{OAuthProviderConfig, OAuthProviderId};

/// Declarative description of an OAuth provider that identifies users by calling a REST user-info
/// endpoint with the access token.
///
/// The plumbing shared by all of them - reading client credentials off the config, parsing the
/// endpoint URLs, exchanging the auth code, issuing the authenticated user-info request - lives in
/// [`SocialProvider`]. Implementors are left with the provider's metadata, the shape of its
/// user-info response and how that maps onto our user model.
///
/// Providers that don't fit, i.e. Apple (claims come from a JWT rather than an API) and OIDC (URLs
/// and scopes come from the config), implement [`OAuthProvider`] directly.
#[async_trait]
pub(crate) trait SocialSpec: Send + Sync + 'static {
  const ID: OAuthProviderId;
  /// Config key and URL path segment, therefore also the name users authenticate against.
  const NAME: &'static str;
  const DISPLAY_NAME: &'static str;

  const AUTH_URL: &'static str;
  const TOKEN_URL: &'static str;
  const USER_API_URL: &'static str;

  const SCOPES: &'static [&'static str];

  /// How client credentials are passed to the token endpoint.
  const AUTH_TYPE: AuthType = AuthType::BasicAuth;

  /// The provider's user-info response.
  ///
  /// Its `TryFrom<Self::User> for ExternalUser` impl is where the mapping onto our user model
  /// lives, next to the struct describing the wire format rather than out here in the metadata.
  type User: DeserializeOwned + Send + TryInto<ExternalUser, Error = AuthError>;

  /// Headers the user-info request needs on top of the bearer token.
  fn user_api_headers(_client_id: &str) -> Vec<(&'static str, String)> {
    return vec![];
  }

  /// Salvages a token response that doesn't comply with RFC-6749. `None` propagates the original
  /// parse error.
  fn recover_token_response(_body: &[u8]) -> Option<Result<TokenResponse, AuthError>> {
    return None;
  }

  /// Resolves the authenticated user.
  ///
  /// Defaults to [`Self::User`]'s own conversion. Override only when the mapping needs a further
  /// request, as Github's does for users who keep their address private.
  async fn map_user(_api: &UserApi<'_>, user: Self::User) -> Result<ExternalUser, AuthError> {
    return user.try_into();
  }

  fn factory() -> OAuthProviderFactory
  where
    Self: Sized,
  {
    return SocialProvider::<Self>::factory();
  }
}

/// Our user model, as far as an external provider can describe it.
///
/// Each provider's user-info struct converts into this via `TryFrom`, which is where you'll find
/// the per-provider mapping.
///
/// Narrower than [`OAuthUser`] on purpose: [`SocialProvider`] fills in the provider id, so a spec
/// can't accidentally claim to be a different provider, and it turns an unverified user into
/// [`AuthError::Unauthorized`], so no spec has to remember that check. Anything a provider doesn't
/// expose is left at its default.
#[derive(Default, Debug)]
pub(crate) struct ExternalUser {
  pub provider_user_id: String,
  pub email: Option<String>,
  pub username: Option<String>,
  /// Whether the provider vouches for the account, e.g. confirmed the email address.
  pub verified: bool,
  pub avatar: Option<String>,
}

/// Payload wrapper for the providers that nest their responses under a `data` key.
#[derive(Debug, Deserialize)]
pub(crate) struct DataEnvelope<T> {
  pub data: T,
}

pub(crate) struct SocialProvider<S: SocialSpec> {
  client: ProviderClient,

  // NOTE: A field rather than `S::USER_API_URL` so tests can point a provider at a fake server.
  user_api_url: String,

  spec: PhantomData<S>,
}

impl<S: SocialSpec> SocialProvider<S> {
  fn new(config: &OAuthProviderConfig) -> Result<Self, OAuthProviderError> {
    return Ok(Self {
      client: ProviderClient::new(config, S::DISPLAY_NAME, S::AUTH_URL, S::TOKEN_URL)?,
      user_api_url: S::USER_API_URL.to_string(),
      spec: PhantomData,
    });
  }

  pub fn factory() -> OAuthProviderFactory {
    return OAuthProviderFactory {
      id: S::ID,
      factory_name: S::NAME,
      factory_display_name: S::DISPLAY_NAME,
      factory: Box::new(|_name: &str, config: &OAuthProviderConfig| {
        Ok(Box::new(Self::new(config)?))
      }),
    };
  }
}

#[async_trait]
impl<S: SocialSpec> OAuthProvider for SocialProvider<S> {
  fn name(&self) -> &str {
    return S::NAME;
  }
  fn display_name(&self) -> &str {
    return S::DISPLAY_NAME;
  }
  fn auth_type(&self) -> AuthType {
    return S::AUTH_TYPE;
  }
  fn oauth_scopes(&self) -> Vec<&str> {
    return S::SCOPES.to_vec();
  }

  fn settings(&self) -> Result<OAuthClientSettings, AuthError> {
    return Ok(self.client.settings());
  }

  fn recover_token_response(&self, body: &[u8]) -> Option<Result<TokenResponse, AuthError>> {
    return S::recover_token_response(body);
  }

  async fn get_user(&self, token_response: &TokenResponse) -> Result<OAuthUser, AuthError> {
    let api = UserApi::new(
      token_response,
      &self.user_api_url,
      S::user_api_headers(self.client.client_id()),
    )?;

    let user = S::map_user(&api, api.get_json::<S::User>(api.user_api_url()).await?).await?;

    // Central so that no spec can forget it: whatever signal a provider offers, `map_user` folds
    // it into `verified` and an account the provider won't vouch for never becomes a local user.
    if !user.verified {
      return Err(AuthError::Unauthorized);
    }

    return Ok(OAuthUser {
      provider_user_id: user.provider_user_id,
      provider_id: S::ID,
      email: user.email,
      username: user.username,
      verified: user.verified,
      avatar: user.avatar,
    });
  }
}

/// Test helpers letting each spec be exercised against a fake provider.
///
/// This is what the indirection buys beyond deduplication: because the user-info endpoint is a
/// field rather than a constant, a spec's request, response parsing and user mapping can be tested
/// without reaching out to the real provider.
#[cfg(test)]
pub(crate) use testing::{USER_API_TEST_PATH, resolve_user, resolve_user_against};

#[cfg(test)]
mod testing {
  use axum::Json;
  use axum::routing::{Router, get};
  use axum_test::{TestServer, TestServerConfig};

  use super::*;
  use crate::auth::oauth::providers::interface::ExtraTokenFields;

  /// Path the fake user-info endpoint is served under.
  pub(crate) const USER_API_TEST_PATH: &str = "/user";

  /// Runs everything `get_user` does, with `routes` standing in for the provider.
  pub(crate) async fn resolve_user_against<S: SocialSpec>(
    routes: Router,
  ) -> Result<OAuthUser, AuthError> {
    let server = TestServer::new_with_config(
      routes,
      TestServerConfig {
        transport: Some(axum_test::Transport::HttpRandomPort),
        ..Default::default()
      },
    );

    let config = OAuthProviderConfig {
      client_id: Some("client_id".to_string()),
      client_secret: Some("client_secret".to_string()),
      ..Default::default()
    };

    let provider = SocialProvider::<S> {
      client: ProviderClient::new(&config, S::DISPLAY_NAME, S::AUTH_URL, S::TOKEN_URL)
        .expect("infallible"),
      user_api_url: server.server_url(USER_API_TEST_PATH).unwrap().to_string(),
      spec: PhantomData,
    };

    return provider
      .get_user(&TokenResponse::new(
        oauth2::AccessToken::new("access_token".to_string()),
        oauth2::basic::BasicTokenType::Bearer,
        ExtraTokenFields { id_token: None },
      ))
      .await;
  }

  /// [`resolve_user_against`] for the common case of a single, static user-info payload.
  pub(crate) async fn resolve_user<S: SocialSpec>(
    user_info: serde_json::Value,
  ) -> Result<OAuthUser, AuthError> {
    let routes = Router::new().route(USER_API_TEST_PATH, get(|| async move { Json(user_info) }));

    return resolve_user_against::<S>(routes).await;
  }
}
