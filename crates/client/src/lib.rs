//! A client library to connect to a TrailBase server via HTTP.
//!
//! TrailBase is a sub-millisecond, open-source application server with type-safe APIs, built-in
//! WASM runtime, realtime, auth, and admin UI built on Rust, SQLite & Wasmtime.

#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use eventsource_stream::Eventsource;
use futures_lite::StreamExt;
use parking_lot::RwLock;
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::borrow::Cow;
use std::sync::Arc;
use thiserror::Error;
use tracing::*;

pub use futures_lite::Stream;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
  #[error("HTTP status: {0}")]
  HttpStatus(StatusCode),

  #[error("RecordSerialization: {0}")]
  RecordSerialization(serde_json::Error),

  #[error("InvalidToken: {0}")]
  InvalidToken(jsonwebtoken::errors::Error),

  #[error("InvalidUrl: {0}")]
  InvalidUrl(url::ParseError),

  // NOTE: This error is leaky but comprehensively unpacking reqwest is unsustainable.
  #[error("Reqwest: {0}")]
  OtherReqwest(reqwest::Error),

  #[cfg(feature = "ws")]
  #[error("WebSocket: {0}")]
  WebSocket(#[from] reqwest_websocket::Error),
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MultiFactorAuthToken {
  mfa_token: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pagination {
  cursor: Option<String>,
  limit: Option<usize>,
  offset: Option<usize>,
}

impl Pagination {
  pub fn new() -> Self {
    return Self::default();
  }

  pub fn with_limit(mut self, limit: impl Into<Option<usize>>) -> Pagination {
    self.limit = limit.into();
    return self;
  }

  pub fn with_cursor(mut self, cursor: impl Into<Option<String>>) -> Pagination {
    self.cursor = cursor.into();
    return self;
  }

  pub fn with_offset(mut self, offset: impl Into<Option<usize>>) -> Pagination {
    self.offset = offset.into();
    return self;
  }
}

type JsonObject = serde_json::value::Map<String, serde_json::Value>;

#[derive(Debug, Clone, Copy, Deserialize_repr, Serialize_repr, PartialEq)]
#[repr(i64)]
pub enum EventErrorStatus {
  /// Unknown or unspecified error.
  Unknown = 0,
  /// Access forbidden.
  Forbidden = 1,
  /// Server-side event-loss, e.g. a buffer ran out of capacity. This does not account for
  /// additional losses that may happen between the TrailBase server and the client. This
  /// needs to be determined client-side based on event `seq` numbers.
  Loss = 2,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum EventPayload {
  Update(JsonObject),
  Insert(JsonObject),
  Delete(JsonObject),
  Error {
    status: EventErrorStatus,
    message: Option<String>,
  },
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ChangeEvent {
  #[serde(flatten)]
  pub event: Arc<EventPayload>,
  pub seq: Option<i64>,
}

impl ChangeEvent {
  fn from_str(msg: &str) -> Result<ChangeEvent, serde_json::Error> {
    return serde_json::from_str::<ChangeEvent>(msg);
  }
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
  fn expand(&self) -> Option<&Vec<&'a str>>;
}

impl<'a, T: RecordId<'a>> ReadArgumentsTrait<'a> for T {
  fn serialized_id(self) -> Cow<'a, str> {
    return self.serialized_id();
  }

  fn expand(&self) -> Option<&Vec<&'a str>> {
    return None;
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReadArguments<'a, T: RecordId<'a>> {
  id: T,
  expand: Option<Vec<&'a str>>,
}

impl<'a, T: RecordId<'a>> ReadArguments<'a, T> {
  pub fn new(id: T) -> Self {
    return Self { id, expand: None };
  }

  pub fn with_expand(mut self, expand: impl AsRef<[&'a str]>) -> Self {
    self.expand = Some(expand.as_ref().to_vec());
    return self;
  }
}

impl<'a, T: RecordId<'a>> ReadArgumentsTrait<'a> for ReadArguments<'a, T> {
  fn serialized_id(self) -> Cow<'a, str> {
    return self.id.serialized_id();
  }

  fn expand(&self) -> Option<&Vec<&'a str>> {
    return self.expand.as_ref();
  }
}

#[async_trait::async_trait]
pub trait Transport {
  async fn fetch(
    &self,
    path: &str,
    headers: HeaderMap,
    method: Method,
    body: Option<Vec<u8>>,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
  ) -> Result<http::Response<reqwest::Body>, Error>;

  #[cfg(feature = "ws")]
  async fn upgrade_ws(
    &self,
    path: &str,
    headers: HeaderMap,
    method: Method,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
  ) -> Result<reqwest_websocket::UpgradeResponse, Error>;
}

pub struct DefaultTransport {
  client: reqwest::Client,
  url: url::Url,
}

impl DefaultTransport {
  pub fn new(url: url::Url) -> Self {
    return Self {
      client: reqwest::Client::new(),
      url,
    };
  }
}

#[async_trait::async_trait]
impl Transport for DefaultTransport {
  async fn fetch(
    &self,
    path: &str,
    headers: HeaderMap,
    method: Method,
    body: Option<Vec<u8>>,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
  ) -> Result<http::Response<reqwest::Body>, Error> {
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
      if let Some(body) = body {
        // let json = serde_json::to_string(body).map_err(Error::RecordSerialization)?;
        // builder = builder.body(json);
        builder = builder.body(body);
      }
      builder.build()?
    };

    return Ok(self.client.execute(request).await?.into());
  }

  #[cfg(feature = "ws")]
  async fn upgrade_ws(
    &self,
    path: &str,
    headers: HeaderMap,
    method: Method,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
  ) -> Result<reqwest_websocket::UpgradeResponse, Error> {
    use reqwest_websocket::Upgrade;

    assert!(path.starts_with("/"));

    let mut url = self.url.clone();
    url.set_path(path);

    if let Some(query_params) = query_params {
      let mut params = url.query_pairs_mut();
      for (key, value) in query_params {
        params.append_pair(key, value);
      }
    }

    return Ok(
      self
        .client
        .request(method, url)
        .headers(headers)
        .upgrade()
        .send()
        .await?,
    );
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

fn decode_auth_token<T: DeserializeOwned + Clone>(token: &str) -> Result<T, Error> {
  return jsonwebtoken::dangerous::insecure_decode::<T>(token)
    .map(|data| data.claims)
    .map_err(Error::InvalidToken);
}

#[derive(Clone)]
pub struct RecordApi {
  client: Arc<ClientState>,
  name: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListArguments<'a> {
  pagination: Pagination,
  order: Option<Vec<&'a str>>,
  filters: Option<ValueOrFilterGroup>,
  expand: Option<Vec<&'a str>>,
  count: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompareOp {
  Equal,
  NotEqual,
  GreaterThanEqual,
  GreaterThan,
  LessThanEqual,
  LessThan,
  Like,
  Regexp,
  StWithin,
  StIntersects,
  StContains,
}

impl CompareOp {
  fn format(&self) -> &'static str {
    return match self {
      Self::Equal => "$eq",
      Self::NotEqual => "$ne",
      Self::GreaterThanEqual => "$gte",
      Self::GreaterThan => "$gt",
      Self::LessThanEqual => "$lte",
      Self::LessThan => "$lt",
      Self::Like => "$like",
      Self::Regexp => "$re",
      Self::StWithin => "@within",
      Self::StIntersects => "@intersects",
      Self::StContains => "@contains",
    };
  }
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Filter {
  pub column: String,
  pub op: Option<CompareOp>,
  pub value: String,
}

impl Filter {
  pub fn new(column: impl Into<String>, op: CompareOp, value: impl Into<String>) -> Self {
    return Self {
      column: column.into(),
      op: Some(op),
      value: value.into(),
    };
  }
}

impl From<Filter> for ValueOrFilterGroup {
  fn from(value: Filter) -> Self {
    return ValueOrFilterGroup::Filter(value);
  }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ValueOrFilterGroup {
  Filter(Filter),
  And(Vec<ValueOrFilterGroup>),
  Or(Vec<ValueOrFilterGroup>),
}

impl<F> From<F> for ValueOrFilterGroup
where
  F: Into<Vec<Filter>>,
{
  fn from(filters: F) -> Self {
    return ValueOrFilterGroup::And(
      filters
        .into()
        .into_iter()
        .map(ValueOrFilterGroup::Filter)
        .collect(),
    );
  }
}

impl<'a> ListArguments<'a> {
  pub fn new() -> Self {
    return ListArguments::default();
  }

  pub fn with_pagination(mut self, pagination: Pagination) -> Self {
    self.pagination = pagination;
    return self;
  }

  pub fn with_order(mut self, order: impl AsRef<[&'a str]>) -> Self {
    self.order = Some(order.as_ref().to_vec());
    return self;
  }

  pub fn with_filters(mut self, filters: impl Into<ValueOrFilterGroup>) -> Self {
    self.filters = Some(filters.into());
    return self;
  }

  pub fn with_expand(mut self, expand: impl AsRef<[&'a str]>) -> Self {
    self.expand = Some(expand.as_ref().to_vec());
    return self;
  }

  pub fn with_count(mut self, count: bool) -> Self {
    self.count = count;
    return self;
  }
}

impl RecordApi {
  pub async fn list<T: DeserializeOwned>(
    &self,
    args: ListArguments<'_>,
  ) -> Result<ListResponse<T>, Error> {
    type Param = (Cow<'static, str>, Cow<'static, str>);
    let mut params: Vec<Param> = vec![];
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

    if let Some(order) = args.order
      && !order.is_empty()
    {
      params.push((Cow::Borrowed("order"), Cow::Owned(to_list(&order))));
    }

    if let Some(expand) = args.expand
      && !expand.is_empty()
    {
      params.push((Cow::Borrowed("expand"), Cow::Owned(to_list(&expand))));
    }

    if args.count {
      params.push((Cow::Borrowed("count"), Cow::Borrowed("true")));
    }

    fn traverse_filters(params: &mut Vec<Param>, path: String, filter: ValueOrFilterGroup) {
      match filter {
        ValueOrFilterGroup::Filter(filter) => {
          if let Some(op) = filter.op {
            params.push((
              Cow::Owned(format!(
                "{path}[{col}][{op}]",
                col = filter.column,
                op = op.format()
              )),
              Cow::Owned(filter.value),
            ));
          } else {
            params.push((
              Cow::Owned(format!("{path}[{col}]", col = filter.column)),
              Cow::Owned(filter.value),
            ));
          }
        }
        ValueOrFilterGroup::And(vec) => {
          for (i, f) in vec.into_iter().enumerate() {
            traverse_filters(params, format!("{path}[$and][{i}]"), f);
          }
        }
        ValueOrFilterGroup::Or(vec) => {
          for (i, f) in vec.into_iter().enumerate() {
            traverse_filters(params, format!("{path}[$or][{i}]"), f);
          }
        }
      }
    }

    if let Some(filters) = args.filters {
      traverse_filters(&mut params, "filter".to_string(), filters);
    }

    let response = self
      .client
      .fetch(
        &format!("/{RECORD_API}/{}", self.name),
        Method::GET,
        None,
        Some(&params),
        /* error_for_status= */ true,
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
        None,
        expand.as_deref(),
        /* error_for_status= */ true,
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
        Some(serde_json::to_vec(&record).map_err(Error::RecordSerialization)?),
        None,
        /* error_for_status= */ true,
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
        Some(serde_json::to_vec(&record).map_err(Error::RecordSerialization)?),
        None,
        /* error_for_status= */ true,
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
        None,
        None,
        /* error_for_status= */ true,
      )
      .await?;

    return Ok(());
  }

  pub async fn subscribe<'a, T: RecordId<'a>>(
    &self,
    id: T,
  ) -> Result<impl Stream<Item = ChangeEvent> + use<T>, Error> {
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
        None,
        None,
        /* error_for_status= */ true,
      )
      .await?;

    return Ok(
      http_body_util::BodyDataStream::new(response.into_body())
        .eventsource()
        .filter_map(|event_or| {
          // QUESTION: Should we instead return a `Stream<Item = Result<ChangeEvent, _>>` to allow
          // for better error handling here.
          if let Ok(event) = event_or {
            return ChangeEvent::from_str(&event.data)
              .map_err(|err| {
                warn!("Failed to parse change event: {}", event.data);
                return err;
              })
              .ok();
          }
          return None;
        }),
    );
  }

  #[cfg(feature = "ws")]
  pub async fn subscribe_ws<'a, T: RecordId<'a>>(
    &self,
    id: T,
  ) -> Result<impl Stream<Item = ChangeEvent> + use<T>, Error> {
    let response = self
      .client
      .upgrade_ws(
        &format!(
          "/{RECORD_API}/{name}/subscribe/{id}",
          name = self.name,
          id = id.serialized_id()
        ),
        Method::GET,
        Some(&[("ws".into(), "true".into())]),
      )
      .await?;

    let websocket = response.into_websocket().await?;

    return Ok(websocket.filter_map(|message| {
      use reqwest_websocket::Message;

      return match message {
        Ok(Message::Text(msg)) => serde_json::from_str::<ChangeEvent>(&msg)
          .map_err(|err| {
            warn!("json error: {err}");
            return err;
          })
          .ok(),
        msg => {
          warn!("unexpected msg: {msg:?}");
          None
        }
      };
    }));
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

struct ClientState {
  transport: Box<dyn Transport + Send + Sync>,
  base_url: url::Url,
  tokens: RwLock<TokenState>,
}

impl ClientState {
  #[inline]
  async fn fetch(
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

#[derive(Clone)]
pub struct Client {
  state: Arc<ClientState>,
}

#[derive(Default)]
pub struct ClientOptions {
  pub tokens: Option<Tokens>,
  pub transport: Option<Box<dyn Transport + Send + Sync>>,
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
    email: &str,
    password: &str,
  ) -> Result<Option<MultiFactorAuthToken>, Error> {
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
        Some(
          serde_json::to_vec(&Credentials { email, password })
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

  pub async fn request_otp(&self, email: &str, redirect_uri: Option<&str>) -> Result<(), Error> {
    #[derive(Serialize)]
    struct Credentials<'a> {
      email: &'a str,
      redirect_uri: Option<&'a str>,
    }

    let _response = self
      .state
      .fetch(
        &format!("/{AUTH_API}/otp/request"),
        Method::POST,
        Some(
          serde_json::to_vec(&Credentials {
            email,
            redirect_uri,
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

#[inline]
async fn json<T: DeserializeOwned>(resp: http::Response<reqwest::Body>) -> Result<T, Error> {
  let full = into_bytes(resp).await?;
  return serde_json::from_slice(&full).map_err(Error::RecordSerialization);
}

#[inline]
async fn into_bytes(resp: http::Response<reqwest::Body>) -> Result<bytes::Bytes, Error> {
  return Ok(
    http_body_util::BodyExt::collect(resp.into_body())
      .await
      .map(|buf| buf.to_bytes())?,
  );
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

  #[test]
  fn parse_change_event_test() {
    let ev0 = ChangeEvent::from_str(
      r#"
        {
          "Error": {
            "status": 1,
            "message": "test"
          },
          "seq": 3
        }"#,
    )
    .unwrap();

    assert_eq!(ev0.seq, Some(3));
    let EventPayload::Error { status, message } = &*ev0.event else {
      panic!("expected error payload, got {:?}", ev0.event);
    };

    assert_eq!(*status, EventErrorStatus::Forbidden);
    assert_eq!(message.as_deref().unwrap(), "test");

    let ev1 = ChangeEvent::from_str(
      r#"
        {
          "Update": {
            "col0": "val0",
            "col1": 4
          }
        }"#,
    )
    .unwrap();

    assert_eq!(ev1.seq, None);
    let EventPayload::Update(obj) = &*ev1.event else {
      panic!("expected update payload, got {:?}", ev1.event);
    };

    assert_eq!(
      serde_json::Value::Object(obj.clone()),
      serde_json::json!({
          "col0": "val0",
          "col1": 4,
      })
    )
  }
}
