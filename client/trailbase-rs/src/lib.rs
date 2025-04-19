//! A client library to connect to a TrailBase server via HTTP.
//!
//! TrailBase is a sub-millisecond, open-source application server with type-safe APIs, built-in
//! JS/ES6/TS runtime, realtime, auth, and admin UI built on Rust, SQLite & V8.

#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use eventsource_stream::Eventsource;
pub use futures::Stream;
use futures::StreamExt;
use parking_lot::RwLock;
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::{Method, StatusCode};
use std::borrow::Cow;
use std::sync::Arc;
use thiserror::Error;
use tracing::*;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

// TODO: Don't leak internals and make this non_exhaustive.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
  #[error("HTTP status: {0}")]
  HttpStatus(StatusCode),

  #[error("MissingRefreshToken")]
  MissingRefreshToken,

  #[error("RecordSerialization: {0}")]
  RecordSerialization(serde_json::Error),

  #[error("InvalidToken: {0}")]
  InvalidToken(jsonwebtoken::errors::Error),

  #[error("InvalidUrl: {0}")]
  InvalidUrl(url::ParseError),

  // NOTE: This error is leaky but comprehensively unpacking reqwest is unsustainable.
  #[error("Reqwest: {0}")]
  OtherReqwest(reqwest::Error),
}

impl From<reqwest::Error> for Error {
  fn from(err: reqwest::Error) -> Self {
    match err.status() {
      Some(code) => Self::HttpStatus(code),
      _ => Self::OtherReqwest(err),
    }
  }
}

/// Represents the currently logged-in user.
#[derive(Clone, Debug)]
pub struct User {
  pub sub: String,
  pub email: String,
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

#[derive(Clone, Debug, Default)]
pub struct Pagination {
  pub cursor: Option<String>,
  pub limit: Option<usize>,
}

impl Pagination {
  pub fn with(limit: impl Into<Option<usize>>, cursor: impl Into<Option<String>>) -> Pagination {
    return Pagination {
      limit: limit.into(),
      cursor: cursor.into(),
    };
  }

  pub fn with_limit(limit: impl Into<Option<usize>>) -> Pagination {
    return Pagination::with(limit, None);
  }

  pub fn with_cursor(cursor: impl Into<Option<String>>) -> Pagination {
    return Pagination::with(None, cursor);
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DbEvent {
  Update(Option<serde_json::Value>),
  Insert(Option<serde_json::Value>),
  Delete(Option<serde_json::Value>),
  Error(String),
}

#[derive(Clone, Debug, Deserialize)]
pub struct ListResponse<T> {
  pub cursor: Option<String>,
  pub total_count: Option<usize>,
  pub records: Vec<T>,
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

pub trait ReadArgumentsTrait<'a> {
  fn serialized_id(self) -> Cow<'a, str>;
  fn expand(&self) -> Option<&'a [&'a str]>;
}

impl<'a, T: RecordId<'a>> ReadArgumentsTrait<'a> for T {
  fn serialized_id(self) -> Cow<'a, str> {
    return self.serialized_id();
  }

  fn expand(&self) -> Option<&'a [&'a str]> {
    return None;
  }
}

#[derive(Debug, Default)]
pub struct ReadArguments<'a, T: RecordId<'a>> {
  pub id: T,
  pub expand: Option<&'a [&'a str]>,
}

impl<'a, T: RecordId<'a>> ReadArgumentsTrait<'a> for ReadArguments<'a, T> {
  fn serialized_id(self) -> Cow<'a, str> {
    return self.id.serialized_id();
  }

  fn expand(&self) -> Option<&'a [&'a str]> {
    return self.expand;
  }
}

struct ThinClient {
  client: reqwest::Client,
  url: url::Url,
}

impl ThinClient {
  async fn fetch<T: Serialize>(
    &self,
    path: &str,
    headers: HeaderMap,
    method: Method,
    body: Option<&T>,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
  ) -> Result<reqwest::Response, Error> {
    assert!(path.starts_with("/"));

    let mut url = self.url.clone();
    url.set_path(path);

    if let Some(query_params) = query_params {
      let mut params = url.query_pairs_mut();
      for (key, value) in query_params {
        params.append_pair(key, value);
      }
    }

    let request = {
      let mut builder = self.client.request(method, url).headers(headers);
      if let Some(ref body) = body {
        let json = serde_json::to_string(body).map_err(Error::RecordSerialization)?;
        builder = builder.body(json);
      }
      builder.build()?
    };

    return Ok(self.client.execute(request).await?);
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

fn decode_auth_token<T: DeserializeOwned>(token: &str) -> Result<T, Error> {
  let decoding_key = jsonwebtoken::DecodingKey::from_secret(&[]);

  // Don't validate the token, we don't have the secret key. Just deserialize the claims/contents.
  let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
  validation.insecure_disable_signature_validation();

  return jsonwebtoken::decode::<T>(token, &decoding_key, &validation)
    .map(|data| data.claims)
    .map_err(Error::InvalidToken);
}

#[derive(Clone)]
pub struct RecordApi {
  client: Arc<ClientState>,
  name: String,
}

#[derive(Default)]
pub struct ListArguments<'a> {
  pub pagination: Pagination,
  pub order: Option<&'a [&'a str]>,
  pub filters: Option<&'a [&'a str]>,
  pub expand: Option<&'a [&'a str]>,
  pub count: bool,
}

impl RecordApi {
  pub async fn list<T: DeserializeOwned>(
    &self,
    args: ListArguments<'_>,
  ) -> Result<ListResponse<T>, Error> {
    let mut params: Vec<(Cow<'static, str>, Cow<'static, str>)> = vec![];
    if let Some(cursor) = args.pagination.cursor {
      params.push((Cow::Borrowed("cursor"), Cow::Owned(cursor)));
    }

    if let Some(limit) = args.pagination.limit {
      params.push((Cow::Borrowed("limit"), Cow::Owned(limit.to_string())));
    }

    #[inline]
    fn to_list(slice: &[&str]) -> String {
      return slice.join(",");
    }

    if let Some(order) = args.order {
      if !order.is_empty() {
        params.push((Cow::Borrowed("order"), Cow::Owned(to_list(order))));
      }
    }

    if let Some(expand) = args.expand {
      if !expand.is_empty() {
        params.push((Cow::Borrowed("expand"), Cow::Owned(to_list(expand))));
      }
    }

    if args.count {
      params.push((Cow::Borrowed("count"), Cow::Borrowed("true")));
    }

    if let Some(filters) = args.filters {
      for filter in filters {
        let Some((name_op, value)) = filter.split_once("=") else {
          panic!("Filter '{filter}' does not match: 'name[op]=value'");
        };

        params.push((
          Cow::Owned(name_op.to_string()),
          Cow::Owned(value.to_string()),
        ));
      }
    }

    let response = self
      .client
      .fetch(
        &format!("/{RECORD_API}/{}", self.name),
        Method::GET,
        None::<&()>,
        Some(&params),
      )
      .await?;

    return json(response).await;
  }

  pub async fn read<'a, T: DeserializeOwned>(
    &self,
    args: impl ReadArgumentsTrait<'a>,
  ) -> Result<T, Error> {
    let expand = args
      .expand()
      .map(|e| vec![(Cow::Borrowed("expand"), Cow::Owned(e.join(",")))]);

    let response = self
      .client
      .fetch(
        &format!(
          "/{RECORD_API}/{name}/{id}",
          name = self.name,
          id = args.serialized_id()
        ),
        Method::GET,
        None::<&()>,
        expand.as_deref(),
      )
      .await?;

    return json(response).await;
  }

  pub async fn create<T: Serialize>(&self, record: T) -> Result<String, Error> {
    return Ok(self.create_impl(record).await?.swap_remove(0));
  }

  pub async fn create_bulk<T: Serialize>(&self, record: &[T]) -> Result<Vec<String>, Error> {
    return self.create_impl(record).await;
  }

  async fn create_impl<T: Serialize>(&self, record: T) -> Result<Vec<String>, Error> {
    let response = self
      .client
      .fetch(
        &format!("/{RECORD_API}/{name}", name = self.name),
        Method::POST,
        Some(&record),
        None,
      )
      .await?;

    #[derive(Deserialize)]
    pub struct RecordIdResponse {
      pub ids: Vec<String>,
    }

    return Ok(json::<RecordIdResponse>(response).await?.ids);
  }

  pub async fn update<'a, T: Serialize>(
    &self,
    id: impl RecordId<'a>,
    record: T,
  ) -> Result<(), Error> {
    self
      .client
      .fetch(
        &format!(
          "/{RECORD_API}/{name}/{id}",
          name = self.name,
          id = id.serialized_id()
        ),
        Method::PATCH,
        Some(&record),
        None,
      )
      .await?;

    return Ok(());
  }

  pub async fn delete<'a>(&self, id: impl RecordId<'a>) -> Result<(), Error> {
    self
      .client
      .fetch(
        &format!(
          "/{RECORD_API}/{name}/{id}",
          name = self.name,
          id = id.serialized_id()
        ),
        Method::DELETE,
        None::<&()>,
        None,
      )
      .await?;

    return Ok(());
  }

  pub async fn subscribe<'a, T: RecordId<'a>>(
    &self,
    id: T,
  ) -> Result<impl Stream<Item = DbEvent> + use<T>, Error> {
    // TODO: Might have to add HeaderValue::from_static("text/event-stream").
    let response = self
      .client
      .fetch(
        &format!(
          "/{RECORD_API}/{name}/subscribe/{id}",
          name = self.name,
          id = id.serialized_id()
        ),
        Method::GET,
        None::<&()>,
        None,
      )
      .await?;

    return Ok(
      response
        .bytes_stream()
        .eventsource()
        .filter_map(|event_or| async {
          if let Ok(event) = event_or {
            if let Ok(db_event) = serde_json::from_str::<DbEvent>(&event.data) {
              return Some(db_event);
            }
          }
          return None;
        }),
    );
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
          error!("Failed to decode auth token.");
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
  tokens: RwLock<TokenState>,
}

impl ClientState {
  #[inline]
  async fn fetch<T: Serialize>(
    &self,
    path: &str,
    method: Method,
    body: Option<&T>,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
  ) -> Result<reqwest::Response, Error> {
    let (mut headers, refresh_token) = self.extract_headers_and_refresh_token_if_exp();
    if let Some(refresh_token) = refresh_token {
      let new_tokens = ClientState::refresh_tokens(&self.client, headers, refresh_token).await?;

      headers = new_tokens.headers.clone();
      *self.tokens.write() = new_tokens;
    }

    return Ok(
      self
        .client
        .fetch(path, headers, method, body, query_params)
        .await?
        .error_for_status()?,
    );
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

  async fn refresh_tokens(
    client: &ThinClient,
    headers: HeaderMap,
    refresh_token: String,
  ) -> Result<TokenState, Error> {
    #[derive(Serialize)]
    struct RefreshRequest<'a> {
      refresh_token: &'a str,
    }

    let response = client
      .fetch(
        &format!("/{AUTH_API}/refresh"),
        headers,
        Method::POST,
        Some(&RefreshRequest {
          refresh_token: &refresh_token,
        }),
        None,
      )
      .await?;

    #[derive(Deserialize)]
    struct RefreshResponse {
      auth_token: String,
      csrf_token: Option<String>,
    }

    let refresh_response: RefreshResponse = json(response).await?;
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
  pub fn new(site: &str, tokens: Option<Tokens>) -> Result<Client, Error> {
    return Ok(Client {
      state: Arc::new(ClientState {
        client: ThinClient {
          client: reqwest::Client::new(),
          url: url::Url::parse(site).map_err(Error::InvalidUrl)?,
        },
        site: site.to_string(),
        tokens: RwLock::new(TokenState::build(tokens.as_ref())),
      }),
    });
  }

  pub fn site(&self) -> String {
    return self.state.site.clone();
  }

  pub fn tokens(&self) -> Option<Tokens> {
    return self.state.tokens.read().state.as_ref().map(|x| x.0.clone());
  }

  pub fn user(&self) -> Option<User> {
    if let Some(state) = &self.state.tokens.read().state {
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
    let Some((headers, refresh_token)) = self.state.extract_headers_refresh_token() else {
      return Err(Error::MissingRefreshToken);
    };

    let new_tokens =
      ClientState::refresh_tokens(&self.state.client, headers, refresh_token).await?;

    *self.state.tokens.write() = new_tokens;
    return Ok(());
  }

  pub async fn login(&self, email: &str, password: &str) -> Result<Tokens, Error> {
    #[derive(Serialize)]
    struct Credentials<'a> {
      email: &'a str,
      password: &'a str,
    }

    let response = self
      .state
      .fetch(
        &format!("/{AUTH_API}/login"),
        Method::POST,
        Some(&Credentials { email, password }),
        None,
      )
      .await?;

    let tokens: Tokens = json(response).await?;
    self.update_tokens(Some(&tokens));
    return Ok(tokens);
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
            Some(&LogoutRequest { refresh_token }),
            None,
          )
          .await
      }
      _ => {
        self
          .state
          .fetch(
            &format!("/{AUTH_API}/logout"),
            Method::GET,
            None::<&()>,
            None,
          )
          .await
      }
    };

    self.update_tokens(None);

    return response_or.map(|_| ());
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

fn now() -> u64 {
  return std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .expect("Duration since epoch")
    .as_secs();
}

#[inline]
async fn json<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T, Error> {
  let full = resp.bytes().await?;
  return serde_json::from_slice(&full).map_err(Error::RecordSerialization);
}

const AUTH_API: &str = "api/auth/v1";
const RECORD_API: &str = "api/records/v1";

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
