use axum::response::sse::Event as SseEvent;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::records::RecordError;

type JsonObject = serde_json::value::Map<String, serde_json::Value>;

#[derive(Debug, Clone, PartialEq)]
pub enum JsonEventPayload {
  Update { value: JsonObject },
  Insert { value: JsonObject },
  Delete { value: JsonObject },
  Error { error: String },
  Ping,
}

fn serialize_raw_json<S>(json: &Option<String>, s: S) -> Result<S::Ok, S::Error>
where
  S: serde::ser::Serializer,
{
  // This should be pretty efficient: it just checks that the string is valid;
  // it doesn't parse it into a new data structure.
  if let Some(json) = json {
    let v: &serde_json::value::RawValue = serde_json::from_str(json).expect("invalid json");
    return v.serialize(s);
  }

  return s.serialize_none();
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedJsonObject {
  #[serde(flatten)]
  json: Box<serde_json::value::RawValue>,
}

impl PartialEq for SerializedJsonObject {
  fn eq(&self, other: &Self) -> bool {
    return self.json.get() == other.json.get();
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PayloadType {
  #[serde(rename = "update")]
  Update,
  #[serde(rename = "insert")]
  Insert,
  #[serde(rename = "delete")]
  Delete,
  #[serde(rename = "error")]
  Error,
  #[serde(rename = "ping")]
  Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
  r#type: PayloadType,
  #[serde(
    skip_serializing_if = "Option::is_none",
    // serialize_with = "serialize_raw_json"
  )]
  value: Option<Box<serde_json::value::RawValue>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  error: Option<String>,
}

impl PartialEq for EventPayload {
  fn eq(&self, other: &Self) -> bool {
    fn get(v: &Option<Box<serde_json::value::RawValue>>) -> Option<&str> {
      return v.as_ref().map(|v| v.get());
    }

    return self.r#type == other.r#type
      && get(&self.value) == get(&other.value)
      && self.error == other.error;
  }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct Event {
  #[serde(flatten)]
  pub payload: Arc<EventPayload>,
  // NOTE: we chose u32 since it should be large enough and can be safely and portably represented
  // in JSON.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub seq: Option<u32>,
}

impl EventPayload {
  pub fn from(value: &JsonEventPayload) -> Self {
    return match value {
      JsonEventPayload::Update { value } => EventPayload {
        r#type: PayloadType::Update,
        value: serde_json::value::to_raw_value(&value)
          .map(|v| v.to_owned())
          .ok(),
        error: None,
      },
      JsonEventPayload::Insert { value } => EventPayload {
        r#type: PayloadType::Insert,
        value: serde_json::value::to_raw_value(&value)
          .map(|v| v.to_owned())
          .ok(),
        error: None,
      },
      JsonEventPayload::Delete { value } => EventPayload {
        r#type: PayloadType::Delete,
        value: serde_json::value::to_raw_value(&value)
          .map(|v| v.to_owned())
          .ok(),
        error: None,
      },
      JsonEventPayload::Error { error } => EventPayload {
        r#type: PayloadType::Error,
        value: None,
        error: Some(error.clone()),
      },
      JsonEventPayload::Ping => EventPayload {
        r#type: PayloadType::Ping,
        value: None,
        error: None,
      },
    };
  }

  pub fn deserialize(&self) -> Result<JsonEventPayload, RecordError> {
    return Ok(match self.r#type {
      PayloadType::Update => JsonEventPayload::Update {
        value: serde_json::from_str(self.value.as_ref().map_or("", |v| v.get())).unwrap(),
      },
      PayloadType::Insert => JsonEventPayload::Insert {
        value: serde_json::from_str(self.value.as_ref().map_or("", |v| v.get())).unwrap(),
      },
      PayloadType::Delete => JsonEventPayload::Delete {
        value: serde_json::from_str(self.value.as_ref().map_or("", |v| v.get())).unwrap(),
      },
      PayloadType::Error => JsonEventPayload::Error {
        error: self.error.as_deref().unwrap_or_default().to_string(),
      },
      PayloadType::Ping => JsonEventPayload::Ping,
    });
  }

  #[inline]
  pub fn into_sse_event(
    self: Arc<EventPayload>,
    seq: Option<u32>,
  ) -> Result<SseEvent, RecordError> {
    if self.r#type == PayloadType::Ping {
      return Ok(SseEvent::default().comment("ping"));
    }

    if let Some(seq) = seq {
      let ev = Event {
        payload: self,
        seq: Some(seq),
      };
      let s = serde_json::to_string(&ev).map_err(|err| RecordError::Internal(err.into()))?;
      return Ok(SseEvent::default().data(&s));
    } else {
      let s = serde_json::to_string(&*self).map_err(|err| RecordError::Internal(err.into()))?;
      return Ok(SseEvent::default().data(&s));
    }
  }

  // #[cfg(feature = "ws")]
  // #[inline]
  // fn into_ws_event(self) -> Result<axum::extract::ws::Message, &'static str> {
  //   let s = match self {
  //     Self::DbEvent(ev) => ev.to_json(None),
  //     Self::Sse(ev) => {
  //       return Err("not sse");
  //     }
  //   };
  //
  //   return Ok(axum::extract::ws::Message::Text(s.into()));
  // }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn serialization_test() {
    let payload0 = EventPayload::from(&JsonEventPayload::Insert {
      value: JsonObject::from_iter([("key0".to_string(), json!("value0"))]),
    });

    let ev0 = Event {
      payload: Arc::new(payload0.clone()),
      seq: None,
    };

    let expected0 = r#"{"type":"insert","value":{"key0":"value0"}}"#;
    let ev0str = serde_json::to_string(&ev0).unwrap();
    assert_eq!(expected0, ev0str);
    assert_eq!(
      expected0,
      serde_json::to_string(&Event {
        payload: Arc::new(payload0.clone()),
        seq: None,
      })
      .unwrap()
    );

    let ev0deserialized: EventPayload = serde_json::from_str(&expected0).unwrap();
    assert_eq!(payload0, ev0deserialized);

    let payload1 = EventPayload::from(&JsonEventPayload::Error {
      error: "boom".to_string(),
    });
    let ev1 = Event {
      payload: Arc::new(payload1),
      seq: Some(11),
    };

    let expected1 = r#"{"type":"error","error":"boom","seq":11}"#;
    let ev1str = serde_json::to_string(&ev1).unwrap();
    assert_eq!(expected1, ev1str);
  }
}
