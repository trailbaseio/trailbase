use eventsource_stream::Eventsource;
use futures_lite::{Stream, StreamExt};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::borrow::Cow;
use std::sync::Arc;
use tracing::*;

use crate::client::ClientState;
use crate::error::Error;
use crate::transport::json;

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

type JsonObject = serde_json::value::Map<String, serde_json::Value>;

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

#[derive(Clone, Debug, Serialize)]
pub enum Operation {
  Create {
    api_name: String,
    value: JsonObject,
  },
  Update {
    api_name: String,
    record_id: String,
    value: JsonObject,
  },
  Delete {
    api_name: String,
    record_id: String,
  },
}

#[derive(Clone, Debug, Deserialize)]
pub enum OperationResult {
  Id(String),
  Error(String),
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
  /// Matches rows where the column IS NULL. Wire: `filter[<col>][$is]=NULL`.
  IsNull,
  /// Matches rows where the column IS NOT NULL. Wire: `filter[<col>][$is]=!NULL`.
  IsNotNull,
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
      Self::IsNull => "$is",
      Self::IsNotNull => "$is",
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

  /// Filter rows where `column` IS NULL. Wire: `filter[<column>][$is]=NULL`.
  pub fn is_null(column: impl Into<String>) -> Self {
    return Self {
      column: column.into(),
      op: Some(CompareOp::IsNull),
      value: String::new(),
    };
  }

  /// Filter rows where `column` IS NOT NULL. Wire: `filter[<column>][$is]=!NULL`.
  pub fn is_not_null(column: impl Into<String>) -> Self {
    return Self {
      column: column.into(),
      op: Some(CompareOp::IsNotNull),
      value: String::new(),
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListArguments<'a> {
  pagination: Pagination,
  order: Option<Vec<&'a str>>,
  filters: Option<ValueOrFilterGroup>,
  expand: Option<Vec<&'a str>>,
  count: bool,
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

#[derive(Clone, Debug, Deserialize)]
pub struct ListResponse<T> {
  pub cursor: Option<String>,
  pub total_count: Option<usize>,
  pub records: Vec<T>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pagination {
  cursor: Option<String>,
  limit: Option<usize>,
  offset: Option<usize>,
}

#[derive(Clone)]
pub struct RecordApi {
  pub(crate) client: Arc<ClientState>,
  pub(crate) name: String,
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
            let value: Cow<'static, str> = match op {
              CompareOp::IsNull => Cow::Borrowed("NULL"),
              CompareOp::IsNotNull => Cow::Borrowed("!NULL"),
              _ => Cow::Owned(filter.value),
            };
            params.push((
              Cow::Owned(format!(
                "{path}[{col}][{op}]",
                col = filter.column,
                op = op.format()
              )),
              value,
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

  pub async fn read<T: DeserializeOwned>(
    &self,
    args: impl ReadArgumentsTrait<'_>,
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

  pub fn create_op<T: Serialize>(&self, record: T) -> Result<Operation, Error> {
    let value = serde_json::to_value(&record).map_err(Error::RecordSerialization)?;
    let serde_json::Value::Object(obj) = value else {
      return Err(Error::InvalidRecord);
    };

    return Ok(Operation::Create {
      api_name: self.name.clone(),
      value: obj,
    });
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

  pub fn update_op<'a, T: Serialize>(
    &self,
    id: impl RecordId<'a>,
    record: T,
  ) -> Result<Operation, Error> {
    let value = serde_json::to_value(&record).map_err(Error::RecordSerialization)?;
    let serde_json::Value::Object(obj) = value else {
      return Err(Error::InvalidRecord);
    };

    return Ok(Operation::Update {
      api_name: self.name.clone(),
      record_id: id.serialized_id().to_string(),
      value: obj,
    });
  }

  pub async fn update<T: Serialize>(&self, id: impl RecordId<'_>, record: T) -> Result<(), Error> {
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

  pub fn delete_op<'a>(&self, id: impl RecordId<'a>) -> Result<Operation, Error> {
    return Ok(Operation::Delete {
      api_name: self.name.clone(),
      record_id: id.serialized_id().to_string(),
    });
  }

  pub async fn delete(&self, id: impl RecordId<'_>) -> Result<(), Error> {
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

const RECORD_API: &str = "api/records/v1";

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn is_null_formats_to_is() {
    assert_eq!(CompareOp::IsNull.format(), "$is");
    assert_eq!(CompareOp::IsNotNull.format(), "$is");
  }

  #[test]
  fn is_null_filter_builder() {
    let f = Filter::is_null("col0");
    assert_eq!(f.column, "col0");
    assert_eq!(f.op, Some(CompareOp::IsNull));
  }

  #[test]
  fn is_null_wire_values() {
    let f_null = Filter::is_null("col0");
    assert_eq!(f_null.op, Some(CompareOp::IsNull));
    assert_eq!(f_null.column, "col0");

    let f_not_null = Filter::is_not_null("col1");
    assert_eq!(f_not_null.op, Some(CompareOp::IsNotNull));
    assert_eq!(f_not_null.column, "col1");

    // Verify the value override logic matches expected wire strings.
    let null_wire: Cow<'static, str> = match f_null.op.unwrap() {
      CompareOp::IsNull => Cow::Borrowed("NULL"),
      CompareOp::IsNotNull => Cow::Borrowed("!NULL"),
      _ => Cow::Owned(f_null.value.clone()),
    };
    let not_null_wire: Cow<'static, str> = match f_not_null.op.unwrap() {
      CompareOp::IsNull => Cow::Borrowed("NULL"),
      CompareOp::IsNotNull => Cow::Borrowed("!NULL"),
      _ => Cow::Owned(f_not_null.value.clone()),
    };

    assert_eq!(null_wire, "NULL");
    assert_eq!(not_null_wire, "!NULL");
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
