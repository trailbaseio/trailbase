use axum::response::sse::Event as SseEvent;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::sync::Arc;

use crate::records::RecordError;

pub type JsonObject = serde_json::value::Map<String, serde_json::Value>;
pub type Record = indexmap::IndexMap<String, trailbase_sqlite::Value>;

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
  /// Failed ot serialize the change event to JSON.
  Serialization = 3,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct EventError {
  pub status: EventErrorStatus,
  pub message: Option<String>,
}

/// Pre-serialized EventPayload with custom Serializer (skips records, which are for filtering
/// only).
#[derive(Debug, Clone)]
pub enum EventPayload {
  Insert {
    json: Box<serde_json::value::RawValue>,
    record: Record,
  },
  Update {
    json: Box<serde_json::value::RawValue>,
    record: Record,
  },
  Delete {
    json: Box<serde_json::value::RawValue>,
    record: Record,
  },
  Error(Box<serde_json::value::RawValue>),
  Ping,
}

impl EventPayload {
  pub fn insert(obj: &JsonObject, record: Record) -> Self {
    return Self::Insert {
      json: to_raw_value(obj),
      record,
    };
  }

  pub fn update(obj: &JsonObject, record: Record) -> Self {
    return Self::Update {
      json: to_raw_value(obj),
      record,
    };
  }

  pub fn delete(obj: &JsonObject, record: Record) -> Self {
    return Self::Delete {
      json: to_raw_value(obj),
      record,
    };
  }

  pub fn error(err: &EventError) -> Self {
    return Self::Error(to_raw_value(err));
  }

  pub fn ping() -> Self {
    return Self::Ping;
  }

  #[inline]
  pub fn into_sse_event(
    self: Arc<EventPayload>,
    seq: Option<i64>,
  ) -> Result<SseEvent, RecordError> {
    return match *self {
      Self::Ping => Ok(SseEvent::default().comment("ping")),
      _ => {
        let ev = ChangeEvent { event: self, seq };
        let s = serde_json::to_string(&ev).map_err(|err| RecordError::Internal(err.into()))?;
        Ok(SseEvent::default().data(&s))
      }
    };
  }

  #[cfg(feature = "ws")]
  #[inline]
  pub fn into_ws_event(self: Arc<EventPayload>) -> Result<axum::extract::ws::Message, RecordError> {
    return match *self {
      Self::Ping => Err(RecordError::Internal("not implemented".into())),
      _ => Ok(axum::extract::ws::Message::Text(
        serde_json::to_string(&*self)
          .map_err(|err| RecordError::Internal(err.into()))?
          .into(),
      )),
    };
  }
}

impl serde::Serialize for EventPayload {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    match *self {
      EventPayload::Insert { ref json, .. } => {
        serializer.serialize_newtype_variant("EventPayload", 0, "Insert", json)
      }
      EventPayload::Update { ref json, .. } => {
        serializer.serialize_newtype_variant("EventPayload", 1, "Update", json)
      }
      EventPayload::Delete { ref json, .. } => {
        serializer.serialize_newtype_variant("EventPayload", 2, "Delete", json)
      }
      EventPayload::Error(ref __field0) => serde::Serializer::serialize_newtype_variant(
        serializer,
        "EventPayload",
        3,
        "Error",
        __field0,
      ),
      EventPayload::Ping => {
        serde::Serializer::serialize_unit_variant(serializer, "EventPayload", 4, "Ping")
      }
    }
  }
}

impl PartialEq for EventPayload {
  fn eq(&self, other: &Self) -> bool {
    return match (self, other) {
      (Self::Insert { json: lhs, .. }, Self::Insert { json: rhs, .. }) => lhs.get() == rhs.get(),
      (Self::Update { json: lhs, .. }, Self::Update { json: rhs, .. }) => lhs.get() == rhs.get(),
      (Self::Delete { json: lhs, .. }, Self::Delete { json: rhs, .. }) => lhs.get() == rhs.get(),
      (Self::Error(lhs), Self::Error(rhs)) => lhs.get() == rhs.get(),
      (Self::Ping, Self::Ping) => true,
      _ => false,
    };
  }
}

/// What's ultimately serialized and sent to clients. Contains a per-subscription sequence number
/// to discover event loss.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChangeEvent {
  #[serde(flatten)]
  event: Arc<EventPayload>,
  // Sequence number is counted by each subscription handler to detect event loss due to network
  // issues.
  //
  // NOTE: Is signed because Avro doesn't support unsigned.
  #[serde(skip_serializing_if = "Option::is_none")]
  seq: Option<i64>,
}

#[cfg(test)]
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum TestJsonEventPayload {
  Update(JsonObject),
  Insert(JsonObject),
  Delete(JsonObject),
  Error {
    status: EventErrorStatus,
    message: Option<String>,
  },
  Ping,
}

#[cfg(test)]
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TestChangeEvent {
  #[serde(flatten)]
  pub event: TestJsonEventPayload,
  pub seq: Option<i64>,
}

// NOTE: to_raw_value should never fail given the limited set of inputs.
#[inline]
#[cfg(not(debug_assertions))]
fn to_raw_value<T>(value: &T) -> Box<serde_json::value::RawValue>
where
  T: ?Sized + Serialize,
{
  return serde_json::value::to_raw_value(value)
    .map(|v| v.to_owned())
    .unwrap_or_default();
}

#[cfg(debug_assertions)]
fn to_raw_value<T>(value: &T) -> Box<serde_json::value::RawValue>
where
  T: ?Sized + Serialize,
{
  return serde_json::value::to_raw_value(value)
    .map(|v| v.to_owned())
    .expect("should never fail for well-defined serde_json::Value");
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn serialization_sse_event_test() {
    {
      let event = ChangeEvent {
        event: Arc::new(EventPayload::delete(
          &JsonObject::from_iter([("foo".to_string(), json!(4))]),
          Default::default(),
        )),
        seq: Some(4),
      };

      let value = serde_json::to_value(&event).unwrap();
      assert_eq!(
        serde_json::json!({
            "Delete": {
                "foo": 4,
            },
            "seq": 4,
        }),
        value
      );
    }

    {
      let event = ChangeEvent {
        event: Arc::new(EventPayload::error(&EventError {
          status: EventErrorStatus::Loss,
          message: Some("test".to_string()),
        })),
        seq: Some(4),
      };

      let expected = serde_json::json!({
          "Error": {
              "status": EventErrorStatus::Loss,
              "message": "test",
          },
          "seq": 4,
      });

      assert_eq!(expected, serde_json::to_value(&event).unwrap());
    }

    {
      let json = r#"
            {
              "Error": {
                "status": 1,
                "message": "test"
              },
              "seq": 3
            }
        "#;

      let test_event = serde_json::from_str::<TestChangeEvent>(json).unwrap();
      assert_eq!(Some(3), test_event.seq);
      assert_eq!(
        TestJsonEventPayload::Error {
          status: EventErrorStatus::Forbidden,
          message: Some("test".to_string()),
        },
        test_event.event
      );
    }
  }
}
