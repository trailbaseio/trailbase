use serde::{Deserialize, Serialize};

type JsonObject = serde_json::value::Map<String, serde_json::Value>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum JsonPayload {
  #[serde(rename = "update")]
  Update { value: JsonObject },
  #[serde(rename = "insert")]
  Insert { value: JsonObject },
  #[serde(rename = "delete")]
  Delete { value: JsonObject },
  #[serde(rename = "error")]
  Error { msg: String },
}

fn serialize_raw_json<S>(json: &str, s: S) -> Result<S::Ok, S::Error>
where
  S: serde::ser::Serializer,
{
  // This should be pretty efficient: it just checks that the string is valid;
  // it doesn't parse it into a new data structure.
  let v: &serde_json::value::RawValue = serde_json::from_str(json).expect("invalid json");
  v.serialize(s)
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct SerializedJsonObject(Box<serde_json::value::RawValue>);
//
// impl PartialEq for SerializedJsonObject {
//   fn eq(&self, other: &Self) -> bool {
//     return self.0.get() == other.0.get();
//   }
// }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum SerializedJsonPayload {
  #[serde(rename = "update")]
  Update {
    #[serde(serialize_with = "serialize_raw_json")]
    value: String,
  },
  #[serde(rename = "insert")]
  Insert {
    #[serde(serialize_with = "serialize_raw_json")]
    value: String,
  },
  #[serde(rename = "delete")]
  Delete {
    #[serde(serialize_with = "serialize_raw_json")]
    value: String,
  },
  #[serde(rename = "error")]
  Error { msg: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Payload {
  Json(JsonPayload),
  SerializedJson(SerializedJsonPayload),
}

impl Payload {
  fn prerender(self) -> Result<Self, serde_json::Error> {
    return Ok(match self {
      Self::Json(payload) => match payload {
        JsonPayload::Update { value } => Self::SerializedJson(SerializedJsonPayload::Update {
          value: serde_json::to_string(&value)?,
        }),
        JsonPayload::Insert { value } => Self::SerializedJson(SerializedJsonPayload::Insert {
          value: serde_json::to_string(&value)?,
        }),
        JsonPayload::Delete { value } => Self::SerializedJson(SerializedJsonPayload::Delete {
          value: serde_json::to_string(&value)?,
        }),
        JsonPayload::Error { msg } => Self::SerializedJson(SerializedJsonPayload::Error { msg }),
      },
      x => x,
    });
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
  #[serde(flatten)]
  payload: Payload,
  // NOTE: we chose u32 since it should be large enough and can be safely and portably represented
  // in JSON.
  #[serde(skip_serializing_if = "Option::is_none")]
  seq: Option<u32>,
}

impl Event {
  fn prerender(self) -> Result<Self, serde_json::Error> {
    return Ok(Self {
      payload: self.payload.prerender()?,
      seq: self.seq,
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn serialization_test() {
    let ev0 = Event {
      payload: Payload::Json(JsonPayload::Insert {
        value: JsonObject::from_iter([("key0".to_string(), json!("value0"))]),
      }),
      seq: None,
    };

    let expected0 = r#"{"type":"insert","value":{"key0":"value0"}}"#;
    let ev0str = serde_json::to_string(&ev0).unwrap();
    assert_eq!(expected0, ev0str);
    assert_eq!(
      expected0,
      serde_json::to_string(&ev0.clone().prerender().unwrap()).unwrap()
    );

    let ev0deserialized: Event = serde_json::from_str(&expected0).unwrap();
    assert!(matches!(ev0deserialized.payload, Payload::Json(_)));
    assert_eq!(ev0, ev0deserialized);

    let ev1 = Event {
      payload: Payload::Json(JsonPayload::Error {
        msg: "boom".to_string(),
      }),
      seq: Some(11),
    };

    let expected1 = r#"{"type":"error","msg":"boom","seq":11}"#;
    let ev1str = serde_json::to_string(&ev1).unwrap();
    assert_eq!(expected1, ev1str);
  }
}
