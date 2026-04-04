use axum::response::sse::Event as SseEvent;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::records::RecordError;

type JsonObject = serde_json::value::Map<String, serde_json::Value>;

#[repr(i64)]
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub enum EventErrorStatus {
  Unknown = 0, 
  Forbidden = 1,
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

// fn serialize_raw_json<S>(json: &Option<String>, s: S) -> Result<S::Ok, S::Error>
// where
//   S: serde::ser::Serializer,
// {
//   // This should be pretty efficient: it just checks that the string is valid;
//   // it doesn't parse it into a new data structure.
//   if let Some(json) = json {
//     let v: &serde_json::value::RawValue = serde_json::from_str(json).expect("invalid json");
//     return v.serialize(s);
//   }
//
//   return s.serialize_none();
// }

#[allow(unused)]
#[derive(Debug, Clone, Serialize)]
pub enum EventPayload {
  Update(Option<Box<serde_json::value::RawValue>>),
  Insert(Option<Box<serde_json::value::RawValue>>),
  Delete(Option<Box<serde_json::value::RawValue>>),
  Error(Option<Box<serde_json::value::RawValue>>),
  Ping,
}

impl PartialEq for EventPayload {
  fn eq(&self, other: &Self) -> bool {
    fn get(v: &Option<Box<serde_json::value::RawValue>>) -> Option<&str> {
      return v.as_ref().map(|v| v.get());
    }

    return match (self, other) {
      (Self::Update(lhs), Self::Update(rhs)) => get(lhs) == get(rhs),
      (Self::Insert(lhs), Self::Insert(rhs)) => get(lhs) == get(rhs),
      (Self::Delete(lhs), Self::Delete(rhs)) => get(lhs) == get(rhs),
      (Self::Error(lhs), Self::Error(rhs)) => get(lhs) == get(rhs),
      (Self::Ping, Self::Ping) => true,
      _ => false,
    };
  }
}

impl EventPayload {
  pub fn from(value: &JsonEventPayload) -> Self {
    return match value {
      JsonEventPayload::Update { value } => EventPayload::Update(
        serde_json::value::to_raw_value(&value)
          .map(|v| v.to_owned())
          .ok(),
      ),
      JsonEventPayload::Insert { value } => EventPayload::Insert(
        serde_json::value::to_raw_value(&value)
          .map(|v| v.to_owned())
          .ok(),
      ),
      JsonEventPayload::Delete { value } => EventPayload::Delete(
        serde_json::value::to_raw_value(&value)
          .map(|v| v.to_owned())
          .ok(),
      ),
      JsonEventPayload::Error { value } => EventPayload::Error(
        serde_json::value::to_raw_value(&value)
          .map(|v| v.to_owned())
          .ok(),
      ),
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

// #[cfg(test)]
// pub fn deserialize_event(ev: Arc<EventPayload>) -> Result<JsonEventPayload, serde_json::Error> {
//   return match *ev {
//     EventPayload::Update(ref v) => Ok(JsonEventPayload::Update {
//       value: serde_json::from_str(v.as_ref().map_or("", |v| v.get()))?,
//     }),
//     EventPayload::Insert(ref v) => Ok(JsonEventPayload::Insert {
//       value: serde_json::from_str(v.as_ref().map_or("", |v| v.get()))?,
//     }),
//     EventPayload::Delete(ref v) => Ok(JsonEventPayload::Delete {
//       value: serde_json::from_str(v.as_ref().map_or("", |v| v.get()))?,
//     }),
//     EventPayload::Error(ref err) => Ok(JsonEventPayload::Error { error: err.clone() }),
//     EventPayload::Ping => Ok(JsonEventPayload::Ping),
//   };
// }

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  // #[test]
  // fn serialization_test() {
  //   let payload0 = EventPayload::from(&JsonEventPayload::Insert {
  //     value: JsonObject::from_iter([("key0".to_string(), json!("value0"))]),
  //   });
  //
  //   let ev0 = ChangeEvent {
  //     payload: Arc::new(payload0.clone()),
  //     seq: None,
  //   };
  //
  //   let expected0 = r#"{"type":"insert","value":{"key0":"value0"}}"#;
  //   let ev0str = serde_json::to_string(&ev0).unwrap();
  //   assert_eq!(expected0, ev0str);
  //   assert_eq!(
  //     expected0,
  //     serde_json::to_string(&ChangeEvent {
  //       payload: Arc::new(payload0.clone()),
  //       seq: None,
  //     })
  //     .unwrap()
  //   );
  //
  //   let ev0deserialized: EventPayload = serde_json::from_str(&expected0).unwrap();
  //   assert_eq!(payload0, ev0deserialized);
  //
  //   let payload1 = EventPayload::from(&JsonEventPayload::Error {
  //     error: "boom".to_string(),
  //   });
  //   let ev1 = ChangeEvent {
  //     payload: Arc::new(payload1),
  //     seq: Some(11),
  //   };
  //
  //   let expected1 = r#"{"type":"error","error":"boom","seq":11}"#;
  //   let ev1str = serde_json::to_string(&ev1).unwrap();
  //   assert_eq!(expected1, ev1str);
  // }

  #[test]
  fn serialization_foo_test() {
    {
      let event = ChangeEvent {
        event: Arc::new(EventPayload::Delete(Some(
          serde_json::value::to_raw_value(&json!({
              "foo": 4,
          }))
          .unwrap(),
        ))),
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
        event: Arc::new(EventPayload::Error(Some(
          serde_json::value::to_raw_value(&json!({
              "status": EventErrorStatus::Loss,
          }))
          .unwrap(),
        ))),
        seq: Some(4),
      };

      let value = serde_json::to_value(&event).unwrap();
      assert_eq!(
        serde_json::json!({
            "Error": {
                "status": EventErrorStatus::Loss,
            },
            "seq": 4,
        }),
        value
      );
    }
  }
}
