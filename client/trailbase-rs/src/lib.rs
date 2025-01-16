#![allow(clippy::needless_return)]

use parking_lot::RwLock;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Method;
use std::borrow::Cow;
use std::sync::Arc;
use thiserror::Error;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Debug, Error)]
pub enum Error {
  #[error("Reqwest: {0}")]
  Reqwest(#[from] reqwest::Error),
  #[error("JSON: {0}")]
  Json(#[from] serde_json::Error),
  #[error("JWT: {0}")]
  Jwt(#[from] jsonwebtoken::errors::Error),
  #[error("Url: {0}")]
  Url(#[from] url::ParseError),
  #[error("Precondition: {0}")]
  Precondition(&'static str),
}

#[derive(Clone, Debug)]
pub struct User {
  pub sub: String,
  pub email: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tokens {
  pub auth_token: String,
  pub refresh_token: Option<String>,
  pub csrf_token: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Pagination {
  pub cursor: Option<String>,
  pub limit: Option<usize>,
}

pub trait RecordId<'a> {
  fn serialized_id(self) -> Cow<'a, str>;
}

impl RecordId<'_> for String {
  fn serialized_id(self) -> Cow<'static, str> {
    return Cow::Owned(self);
  }
}

impl<'a> RecordId<'a> for &'a String {
  fn serialized_id(self) -> Cow<'a, str> {
    return Cow::Borrowed(self);
  }
}

impl<'a> RecordId<'a> for &'a str {
  fn serialized_id(self) -> Cow<'a, str> {
    return Cow::Borrowed(self);
  }
}

impl RecordId<'_> for i64 {
  fn serialized_id(self) -> Cow<'static, str> {
    return Cow::Owned(self.to_string());
  }
}
struct ThinClient {
  client: reqwest::Client,
  site: String,
}

impl ThinClient {
  async fn fetch(
    &self,
    path: String,
    headers: HeaderMap,
    method: Method,
    body: Option<serde_json::Value>,
    query_params: Option<Vec<(String, String)>>,
  ) -> Result<reqwest::Response, Error> {
    if path.starts_with("/") {
      return Err(Error::Precondition("path must not start with '/'"));
    }

    let mut url = url::Url::parse(&format!("{}/{path}", self.site))?;

    if let Some(query_params) = query_params {
      for (key, value) in query_params {
        url.query_pairs_mut().append_pair(&key, &value);
      }
    }

    let mut builder = self.client.request(method, url).headers(headers);

    if let Some(body) = body {
      builder = builder.body(serde_json::to_string(&body)?);
    }

    return Ok(self.client.execute(builder.build()?).await?);
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct JwtTokenClaims {
  sub: String,
  iat: i64,
  exp: i64,
  email: String,
  csrf_token: String,
}

pub fn decode_auth_token<T: DeserializeOwned>(token: &str) -> Result<T, Error> {
  let decoding_key = jsonwebtoken::DecodingKey::from_secret(&[]);

  // Don't validate the token, we don't have the secret key. Just deserialize the claims/contents.
  let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
  validation.insecure_disable_signature_validation();

  return Ok(jsonwebtoken::decode::<T>(token, &decoding_key, &validation).map(|data| data.claims)?);
}

#[derive(Clone)]
pub struct RecordApi {
  client: Arc<ClientState>,
  name: String,
}

impl RecordApi {
  // TODO: add subscription APIs.

  pub async fn list<T: DeserializeOwned>(
    &self,
    pagination: Option<Pagination>,
    order: Option<&[&str]>,
    filters: Option<&[&str]>,
  ) -> Result<Vec<T>, Error> {
    let mut params: Vec<(String, String)> = vec![];
    if let Some(pagination) = pagination {
      if let Some(cursor) = pagination.cursor {
        params.push(("cursor".to_string(), cursor));
      }

      if let Some(limit) = pagination.limit {
        params.push(("limit".to_string(), limit.to_string()));
      }
    }

    if let Some(order) = order {
      params.push(("order".to_string(), order.join(",")));
    }

    if let Some(filters) = filters {
      for filter in filters {
        let Some((name_op, value)) = filter.split_once("=") else {
          panic!("Filter '{filter}' does not match: 'name[op]=value'");
        };

        params.push((name_op.to_string(), value.to_string()));
      }
    }

    let response = self
      .client
      .fetch(
        format!("{RECORD_API}/{}", self.name),
        Method::GET,
        None,
        Some(params),
      )
      .await?;

    return Ok(response.json().await?);
  }

  pub async fn read<'a, T: DeserializeOwned>(&self, id: impl RecordId<'a>) -> Result<T, Error> {
    let response = self
      .client
      .fetch(
        format!(
          "{RECORD_API}/{name}/{id}",
          name = self.name,
          id = id.serialized_id()
        ),
        Method::GET,
        None,
        None,
      )
      .await?;

    return Ok(response.json().await?);
  }

  pub async fn create<T: Serialize>(&self, record: T) -> Result<String, Error> {
    let response = self
      .client
      .fetch(
        format!("{RECORD_API}/{name}", name = self.name),
        Method::POST,
        Some(serde_json::to_value(record)?),
        None,
      )
      .await?;

    #[derive(Deserialize)]
    pub struct RecordIdResponse {
      pub id: String,
    }

    return Ok(response.json::<RecordIdResponse>().await?.id);
  }

  pub async fn update<'a, T: Serialize>(
    &self,
    id: impl RecordId<'a>,
    record: T,
  ) -> Result<(), Error> {
    self
      .client
      .fetch(
        format!(
          "{RECORD_API}/{name}/{id}",
          name = self.name,
          id = id.serialized_id()
        ),
        Method::PATCH,
        Some(serde_json::to_value(record)?),
        None,
      )
      .await?;

    return Ok(());
  }

  pub async fn delete<'a>(&self, id: impl RecordId<'a>) -> Result<(), Error> {
    self
      .client
      .fetch(
        format!(
          "{RECORD_API}/{name}/{id}",
          name = self.name,
          id = id.serialized_id()
        ),
        Method::DELETE,
        None,
        None,
      )
      .await?;

    return Ok(());
  }
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
        let Ok(jwt_token) = decode_auth_token::<JwtTokenClaims>(&tokens.auth_token) else {
          log::error!("Failed to decode auth token.");
          return None;
        };
        return Some((tokens.clone(), jwt_token));
      }),
      headers,
    };
  }
}

struct ClientState {
  client: ThinClient,
  site: String,
  token_state: RwLock<TokenState>,
}

impl ClientState {
  #[inline]
  async fn fetch(
    &self,
    url: String,
    method: Method,
    body: Option<serde_json::Value>,
    query_params: Option<Vec<(String, String)>>,
  ) -> Result<reqwest::Response, Error> {
    let (needs_refetch, mut headers) = {
      let token_state = self.token_state.read();
      (
        Self::should_refresh(&token_state),
        token_state.headers.clone(),
      )
    };

    if needs_refetch {
      let refresh_token = {
        let token_state = self.token_state.read();
        let Some(ref refresh_token) = token_state
          .state
          .as_ref()
          .and_then(|s| s.0.refresh_token.clone())
        else {
          return Err(Error::Precondition("Missing refresh token"));
        };
        refresh_token.clone()
      };

      let new_token_state =
        ClientState::refresh_tokens(&self.client, headers, refresh_token).await?;

      let new_headers = new_token_state.headers.clone();

      *self.token_state.write() = new_token_state;

      headers = new_headers;
    }

    return self
      .client
      .fetch(url, headers, method, body, query_params)
      .await;
  }

  #[inline]
  fn should_refresh(token_state: &TokenState) -> bool {
    let now = now();
    if let Some(ref state) = token_state.state {
      return state.1.exp - 60 < now as i64;
    }
    return false;
  }

  fn extract_refresh_token_and_headers(
    token_state: &TokenState,
  ) -> Result<(String, HeaderMap), Error> {
    let Some(ref state) = token_state.state else {
      return Err(Error::Precondition("Not logged int?"));
    };

    let Some(ref refresh_token) = state.0.refresh_token else {
      return Err(Error::Precondition("Missing refresh token"));
    };

    return Ok((refresh_token.clone(), token_state.headers.clone()));
  }

  async fn refresh_tokens(
    client: &ThinClient,
    headers: HeaderMap,
    refresh_token: String,
  ) -> Result<TokenState, Error> {
    let response = client
      .fetch(
        format!("{AUTH_API}/refresh"),
        headers,
        Method::POST,
        Some(serde_json::json!({
          "refresh_token": refresh_token,
        })),
        None,
      )
      .await?;

    #[derive(Deserialize)]
    struct RefreshResponse {
      auth_token: String,
      csrf_token: Option<String>,
    }

    let refresh_response: RefreshResponse = response.json().await?;
    return Ok(TokenState::build(Some(&Tokens {
      auth_token: refresh_response.auth_token,
      refresh_token: Some(refresh_token),
      csrf_token: refresh_response.csrf_token,
    })));
  }
}

#[derive(Clone)]
pub struct Client {
  state: Arc<ClientState>,
}

impl Client {
  pub fn new(site: &str, tokens: Option<Tokens>) -> Client {
    return Client {
      state: Arc::new(ClientState {
        client: ThinClient {
          client: reqwest::Client::new(),
          site: site.to_string(),
        },
        site: site.to_string(),
        token_state: RwLock::new(TokenState::build(tokens.as_ref())),
      }),
    };
  }

  pub fn site(&self) -> String {
    return self.state.site.clone();
  }

  pub fn tokens(&self) -> Option<Tokens> {
    return self
      .state
      .token_state
      .read()
      .state
      .as_ref()
      .map(|x| x.0.clone());
  }

  pub fn user(&self) -> Option<User> {
    if let Some(state) = &self.state.token_state.read().state {
      return Some(User {
        sub: state.1.sub.clone(),
        email: state.1.email.clone(),
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
    let (refresh_token, headers) =
      ClientState::extract_refresh_token_and_headers(&self.state.token_state.read())?;
    let new_token_state =
      ClientState::refresh_tokens(&self.state.client, headers, refresh_token).await?;

    *self.state.token_state.write() = new_token_state;
    return Ok(());
  }

  pub async fn login(&self, email: &str, password: &str) -> Result<Tokens, Error> {
    let response = self
      .state
      .fetch(
        format!("{AUTH_API}/login"),
        Method::POST,
        Some(serde_json::json!({
          "email": email,
          "password": password,
        })),
        None,
      )
      .await?;

    let tokens: Tokens = response.json().await?;
    self.update_tokens(Some(&tokens));
    return Ok(tokens);
  }

  pub async fn logout(&self) -> Result<(), Error> {
    let refresh_token: Option<String> = self
      .state
      .token_state
      .read()
      .state
      .as_ref()
      .and_then(|s| s.0.refresh_token.clone());
    if let Some(refresh_token) = refresh_token {
      self
        .state
        .fetch(
          format!("{AUTH_API}/logout"),
          Method::POST,
          Some(serde_json::json!({"refresh_token": refresh_token})),
          None,
        )
        .await?;
    } else {
      self
        .state
        .fetch(format!("{AUTH_API}/logout"), Method::GET, None, None)
        .await?;
    }

    self.update_tokens(None);

    return Ok(());
  }

  fn update_tokens(&self, tokens: Option<&Tokens>) -> TokenState {
    let state = TokenState::build(tokens);

    *self.state.token_state.write() = state.clone();
    // _authChange?.call(this, state.state?.$1);

    if let Some(ref s) = state.state {
      let now = now();
      if s.1.exp < now as i64 {
        log::warn!("Token expired");
      }
    }

    return state;
  }
}

fn build_headers(tokens: Option<&Tokens>) -> HeaderMap {
  let mut base = HeaderMap::new();
  base.insert("Content-Type", HeaderValue::from_static("application/json"));

  if let Some(tokens) = tokens {
    if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", tokens.auth_token)) {
      base.insert("Authorization", value);
    } else {
      log::error!("Failed to build bearer token.");
    }

    if let Some(ref refresh) = tokens.refresh_token {
      if let Ok(value) = HeaderValue::from_str(refresh) {
        base.insert("Refresh-Token", value);
      } else {
        log::error!("Failed to build refresh token header.");
      }
    }

    if let Some(ref csrf) = tokens.csrf_token {
      if let Ok(value) = HeaderValue::from_str(csrf) {
        base.insert("CSRF-Token", value);
      } else {
        log::error!("Failed to build refresh token header.");
      }
    }
  }

  return base;
}

fn now() -> u64 {
  return std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .expect("Duration since epoch")
    .as_secs();
}

const AUTH_API: &str = "api/auth/v1";
const RECORD_API: &str = "api/records/v1";

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn is_send_test() {
    let client = Client::new("http://127.0.0.1:4000", None);

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
