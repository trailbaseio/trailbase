use parking_lot::RwLock;
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;
use tracing::*;

use crate::error::Error;
use crate::record_api::RecordApi;
use crate::transport::{DefaultTransport, Transport, json};

/// Represents the currently logged-in user.
#[derive(Clone, Debug)]
pub struct User {
  pub sub: String,
  pub email: Option<String>,
  pub username: Option<String>,
}

/// Holds the tokens minted by the server on login.
///
/// It is also the exact JSON serialization format.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tokens {
  pub auth_token: String,
  pub refresh_token: Option<String>,
  pub csrf_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MultiFactorAuthToken {
  mfa_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct JwtTokenClaims {
  sub: String,
  iat: i64,
  exp: i64,
  email: Option<String>,
  username: Option<String>,
  csrf_token: String,
}

fn decode_auth_token<T: DeserializeOwned + Clone>(token: &str) -> Result<T, Error> {
  return jsonwebtoken::dangerous::insecure_decode::<T>(token)
    .map(|data| data.claims)
    .map_err(Error::InvalidToken);
}

#[derive(Clone, Debug)]
struct TokenState {
  state: Option<(Tokens, JwtTokenClaims)>,
  headers: HeaderMap,
}

impl TokenState {
  fn build(tokens: Option<&Tokens>) -> TokenState {
    let headers = build_headers(tokens);
    return TokenState {
      state: tokens.and_then(|tokens| {
        return match decode_auth_token::<JwtTokenClaims>(&tokens.auth_token) {
          Ok(jwt_token) => Some((tokens.clone(), jwt_token)),
          Err(err) => {
            error!("Failed to decode auth token: {err}");
            None
          }
        };
      }),
      headers,
    };
  }
}

pub(crate) struct ClientState {
  transport: Box<dyn Transport + Send + Sync>,
  base_url: url::Url,
  tokens: RwLock<TokenState>,
}

impl ClientState {
  #[inline]
  pub(crate) async fn fetch(
    &self,
    path: &str,
    method: Method,
    body: Option<Vec<u8>>,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
    error_for_status: bool,
  ) -> Result<http::Response<reqwest::Body>, Error> {
    let (mut headers, refresh_token) = self.extract_headers_and_refresh_token_if_exp();
    if let Some(refresh_token) = refresh_token {
      let new_tokens = refresh_tokens_impl(&*self.transport, headers, refresh_token).await?;

      headers = new_tokens.headers.clone();
      *self.tokens.write() = new_tokens;
    }

    let response = self
      .transport
      .fetch(path, headers, method, body, query_params)
      .await?;

    if error_for_status {
      return error_for_status_unpack(response);
    }
    return Ok(response);
  }

  #[cfg(feature = "ws")]
  #[inline]
  async fn upgrade_ws(
    &self,
    path: &str,
    method: Method,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
  ) -> Result<reqwest_websocket::UpgradeResponse, Error> {
    let (mut headers, refresh_token) = self.extract_headers_and_refresh_token_if_exp();
    if let Some(refresh_token) = refresh_token {
      let new_tokens = refresh_tokens_impl(&*self.transport, headers, refresh_token).await?;

      headers = new_tokens.headers.clone();
      *self.tokens.write() = new_tokens;
    }

    return self
      .transport
      .upgrade_ws(path, headers, method, query_params)
      .await;
  }

  #[inline]
  fn extract_headers_and_refresh_token_if_exp(&self) -> (HeaderMap, Option<String>) {
    #[inline]
    fn should_refresh(jwt: &JwtTokenClaims) -> bool {
      return jwt.exp - 60 < now() as i64;
    }

    let tokens = self.tokens.read();
    let headers = tokens.headers.clone();
    return match tokens.state {
      Some(ref state) if should_refresh(&state.1) => (headers, state.0.refresh_token.clone()),
      _ => (headers, None),
    };
  }

  fn extract_headers_refresh_token(&self) -> Option<(HeaderMap, String)> {
    let tokens = self.tokens.read();
    let state = tokens.state.as_ref()?;

    if let Some(ref refresh_token) = state.0.refresh_token {
      return Some((tokens.headers.clone(), refresh_token.clone()));
    }
    return None;
  }
}

#[derive(Default)]
pub struct PromoteOptions {
  pub password: String,
  pub email: Option<String>,
  pub username: Option<String>,
}

#[derive(Default)]
pub struct ClientOptions {
  pub tokens: Option<Tokens>,
  pub transport: Option<Box<dyn Transport + Send + Sync>>,
}

#[derive(Clone)]
pub struct Client {
  state: Arc<ClientState>,
}

impl Client {
  pub fn new(
    base_url: impl TryInto<url::Url, Error = url::ParseError>,
    opts: Option<ClientOptions>,
  ) -> Result<Client, Error> {
    let opts = opts.unwrap_or_default();
    let base_url = base_url.try_into().map_err(Error::InvalidUrl)?;
    return Ok(Client {
      state: Arc::new(ClientState {
        transport: opts.transport.unwrap_or_else(|| {
          return Box::new(DefaultTransport::new(base_url.clone()));
        }),
        base_url,
        tokens: RwLock::new(TokenState::build(opts.tokens.as_ref())),
      }),
    });
  }

  pub fn base_url(&self) -> &url::Url {
    return &self.state.base_url;
  }

  pub fn tokens(&self) -> Option<Tokens> {
    return self.state.tokens.read().state.as_ref().map(|x| x.0.clone());
  }

  pub fn user(&self) -> Option<User> {
    if let Some(state) = &self.state.tokens.read().state {
      return Some(User {
        sub: state.1.sub.clone(),
        email: state.1.email.clone(),
        username: state.1.username.clone(),
      });
    }
    return None;
  }

  pub fn records(&self, api_name: &str) -> RecordApi {
    return RecordApi {
      client: self.state.clone(),
      name: api_name.to_string(),
    };
  }

  pub async fn refresh(&self) -> Result<(), Error> {
    let Some((headers, refresh_token)) = self.state.extract_headers_refresh_token() else {
      // Not logged in - nothing to do.
      return Ok(());
    };

    let new_tokens = refresh_tokens_impl(&*self.state.transport, headers, refresh_token).await?;

    *self.state.tokens.write() = new_tokens;
    return Ok(());
  }

  pub async fn login(
    &self,
    email_or_username: &str,
    password: &str,
  ) -> Result<Option<MultiFactorAuthToken>, Error> {
    #[derive(Serialize)]
    struct Credentials<'a> {
      email_or_username: &'a str,
      password: &'a str,
    }

    let response = self
      .state
      .fetch(
        &format!("/{AUTH_API}/login"),
        Method::POST,
        Some(
          serde_json::to_vec(&Credentials {
            email_or_username,
            password,
          })
          .map_err(Error::RecordSerialization)?,
        ),
        None,
        /* error_for_status= */ false,
      )
      .await?;

    if response.status() == StatusCode::FORBIDDEN {
      let mfa_token: MultiFactorAuthToken = json(response).await?;
      return Ok(Some(mfa_token));
    }

    let tokens: Tokens = json(error_for_status_unpack(response)?).await?;
    self.update_tokens(Some(&tokens));

    return Ok(None);
  }

  pub async fn login_second(
    &self,
    mfa_token: &MultiFactorAuthToken,
    totp_code: &str,
  ) -> Result<(), Error> {
    #[derive(Serialize)]
    struct Credentials<'a> {
      mfa_token: &'a str,
      totp: &'a str,
    }

    let response = self
      .state
      .fetch(
        &format!("/{AUTH_API}/login_mfa"),
        Method::POST,
        Some(
          serde_json::to_vec(&Credentials {
            mfa_token: &mfa_token.mfa_token,
            totp: totp_code,
          })
          .map_err(Error::RecordSerialization)?,
        ),
        None,
        /* error_for_status= */ true,
      )
      .await?;

    let tokens: Tokens = json(error_for_status_unpack(response)?).await?;
    self.update_tokens(Some(&tokens));

    return Ok(());
  }

  pub async fn request_otp(&self, email_or_username: &str) -> Result<(), Error> {
    #[derive(Serialize)]
    struct Credentials<'a> {
      email_or_username: &'a str,
      redirect_uri: Option<&'a str>,
    }

    let _response = self
      .state
      .fetch(
        &format!("/{AUTH_API}/otp/request"),
        Method::POST,
        Some(
          serde_json::to_vec(&Credentials {
            email_or_username,
            redirect_uri: None,
          })
          .map_err(Error::RecordSerialization)?,
        ),
        None,
        /* error_for_status= */ true,
      )
      .await?;

    return Ok(());
  }

  pub async fn login_otp(&self, email: &str, code: &str) -> Result<(), Error> {
    #[derive(Serialize)]
    struct Credentials<'a> {
      email: &'a str,
      code: &'a str,
    }

    let response = self
      .state
      .fetch(
        &format!("/{AUTH_API}/otp/login"),
        Method::POST,
        Some(serde_json::to_vec(&Credentials { email, code }).map_err(Error::RecordSerialization)?),
        None,
        /* error_for_status= */ true,
      )
      .await?;

    let tokens: Tokens = json(error_for_status_unpack(response)?).await?;
    self.update_tokens(Some(&tokens));

    return Ok(());
  }

  pub async fn login_anonymously(&self) -> Result<(), Error> {
    let response = self
      .state
      .fetch(
        &format!("/{AUTH_API}/login_anonymous"),
        Method::POST,
        Some(b"{}".into()),
        None,
        /* error_for_status= */ true,
      )
      .await?;

    let tokens: Tokens = json(error_for_status_unpack(response)?).await?;
    self.update_tokens(Some(&tokens));

    return Ok(());
  }

  pub async fn logout(&self) -> Result<(), Error> {
    #[derive(Serialize)]
    struct LogoutRequest {
      refresh_token: String,
    }

    let response_or = match self.state.extract_headers_refresh_token() {
      Some((_headers, refresh_token)) => {
        self
          .state
          .fetch(
            &format!("/{AUTH_API}/logout"),
            Method::POST,
            Some(
              serde_json::to_vec(&LogoutRequest { refresh_token })
                .map_err(Error::RecordSerialization)?,
            ),
            None,
            /* error_for_status= */ true,
          )
          .await
      }
      _ => {
        self
          .state
          .fetch(
            &format!("/{AUTH_API}/logout"),
            Method::GET,
            None,
            None,
            /* error_for_status= */ true,
          )
          .await
      }
    };

    self.update_tokens(None);

    return response_or.map(|_| ());
  }

  pub async fn promote_anonymous(&self, opts: PromoteOptions) -> Result<(), Error> {
    #[derive(Serialize)]
    struct Request<'a> {
      new_password: &'a str,
      new_email: Option<&'a str>,
      new_username: Option<&'a str>,
    }

    self
      .state
      .fetch(
        &format!("/{AUTH_API}/promote_anonymous"),
        Method::POST,
        Some(
          serde_json::to_vec(&Request {
            new_password: &opts.password,
            new_email: opts.email.as_deref(),
            new_username: opts.username.as_deref(),
          })
          .map_err(Error::RecordSerialization)?,
        ),
        None,
        /* error_for_status= */ true,
      )
      .await?;

    return Ok(());
  }

  fn update_tokens(&self, tokens: Option<&Tokens>) -> TokenState {
    let state = TokenState::build(tokens);

    *self.state.tokens.write() = state.clone();
    // _authChange?.call(this, state.state?.$1);

    if let Some(ref s) = state.state {
      let now = now();
      if s.1.exp < now as i64 {
        warn!("Token expired");
      }
    }

    return state;
  }
}

fn build_headers(tokens: Option<&Tokens>) -> HeaderMap {
  let mut base = HeaderMap::with_capacity(5);
  base.insert(
    header::CONTENT_TYPE,
    HeaderValue::from_static("application/json"),
  );

  if let Some(tokens) = tokens {
    if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", tokens.auth_token)) {
      base.insert(header::AUTHORIZATION, value);
    } else {
      error!("Failed to build bearer token.");
    }

    if let Some(ref refresh) = tokens.refresh_token {
      if let Ok(value) = HeaderValue::from_str(refresh) {
        base.insert("Refresh-Token", value);
      } else {
        error!("Failed to build refresh token header.");
      }
    }

    if let Some(ref csrf) = tokens.csrf_token {
      if let Ok(value) = HeaderValue::from_str(csrf) {
        base.insert("CSRF-Token", value);
      } else {
        error!("Failed to build refresh token header.");
      }
    }
  }

  return base;
}

async fn refresh_tokens_impl(
  transport: &(dyn Transport + Send + Sync),
  headers: HeaderMap,
  refresh_token: String,
) -> Result<TokenState, Error> {
  #[derive(Serialize)]
  struct RefreshRequest<'a> {
    refresh_token: &'a str,
  }

  // NOTE: Do not use `ClientState::fetch`, which may do token refreshing to avoid loops.
  let response = transport
    .fetch(
      &format!("/{AUTH_API}/refresh"),
      headers,
      Method::POST,
      Some(
        serde_json::to_vec(&RefreshRequest {
          refresh_token: &refresh_token,
        })
        .map_err(Error::RecordSerialization)?,
      ),
      None,
    )
    .await?;

  return match response.status() {
    StatusCode::OK => {
      #[derive(Deserialize)]
      struct RefreshResponse {
        auth_token: String,
        csrf_token: Option<String>,
      }

      let refresh_response: RefreshResponse = json(response).await?;

      Ok(TokenState::build(Some(&Tokens {
        auth_token: refresh_response.auth_token,
        refresh_token: Some(refresh_token),
        csrf_token: refresh_response.csrf_token,
      })))
    }
    StatusCode::UNAUTHORIZED => Ok(TokenState::build(None)),
    status => Err(Error::HttpStatus(status)),
  };
}

fn now() -> u64 {
  return std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .expect("Duration since epoch")
    .as_secs();
}

fn error_for_status_unpack(
  resp: http::Response<reqwest::Body>,
) -> Result<http::Response<reqwest::Body>, Error> {
  let status = resp.status();
  if status.is_client_error() || status.is_server_error() {
    return Err(Error::HttpStatus(status));
  }
  return Ok(resp);
}

const AUTH_API: &str = "api/auth/v1";

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn is_send_test() {
    let client = Client::new("http://127.0.0.1:4000", None).unwrap();

    let api = client.records("simple_strict_table");

    for _ in 0..2 {
      let api = api.clone();
      tokio::spawn(async move {
        // This would not compile if locks would be held across async function calls.
        let response = api.read::<serde_json::Value>(0).await;
        assert!(response.is_err());
      })
      .await
      .unwrap();
    }
  }
}
