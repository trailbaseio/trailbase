use axum::response::sse::Event as SseEvent;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::sync::Arc;

use crate::records::RecordError;

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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct EventError {
  pub status: EventErrorStatus,
  pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsonEventPayload {
  Update { value: JsonObject },
  Insert { value: JsonObject },
  Delete { value: JsonObject },
  Error { value: EventError },
  Ping,
}

/// EventPayload represents a serialized JsonEventPayload.
#[derive(Debug, Clone, Serialize)]
pub enum EventPayload {
  Update(Box<serde_json::value::RawValue>),
  Insert(Box<serde_json::value::RawValue>),
  Delete(Box<serde_json::value::RawValue>),
  Error(Box<serde_json::value::RawValue>),
  Ping,
}

impl PartialEq for EventPayload {
  fn eq(&self, other: &Self) -> bool {
    return match (self, other) {
      (Self::Update(lhs), Self::Update(rhs)) => lhs.get() == rhs.get(),
      (Self::Insert(lhs), Self::Insert(rhs)) => lhs.get() == rhs.get(),
      (Self::Delete(lhs), Self::Delete(rhs)) => lhs.get() == rhs.get(),
      (Self::Error(lhs), Self::Error(rhs)) => lhs.get() == rhs.get(),
      (Self::Ping, Self::Ping) => true,
      _ => false,
    };
  }
}

impl EventPayload {
  pub fn from(value: &JsonEventPayload) -> Self {
    // NOTE: to_raw_value should never fail.
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

    return match value {
      JsonEventPayload::Update { value } => EventPayload::Update(to_raw_value(&value)),
      JsonEventPayload::Insert { value } => EventPayload::Insert(to_raw_value(value)),
      JsonEventPayload::Delete { value } => EventPayload::Delete(to_raw_value(value)),
      JsonEventPayload::Error { value } => EventPayload::Error(to_raw_value(value)),
      JsonEventPayload::Ping => EventPayload::Ping,
    };
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

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChangeEvent {
  #[serde(flatten)]
  event: Arc<EventPayload>,
  // NOTE: Because unsigned isn't supported by Avro.
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

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn serialization_sse_event_test() {
    {
      let event = ChangeEvent {
        event: Arc::new(EventPayload::from(&JsonEventPayload::Delete {
          value: JsonObject::from_iter([("foo".to_string(), json!(4))]),
        })),
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
        event: Arc::new(EventPayload::from(&JsonEventPayload::Error {
          value: EventError {
            status: EventErrorStatus::Loss,
            message: Some("test".to_string()),
          },
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
