pub type JsonObject = serde_json::value::Map<String, serde_json::Value>;

// We have our own Value representation for JSON serialization only to reduce allocations.
#[derive(Clone, Default)]
pub enum Value<'ctx> {
  #[default]
  Null,
  Bool(bool),
  Number(serde_json::Number),
  String(std::borrow::Cow<'ctx, str>),
  Array(Vec<Value<'ctx>>),
  // Two object representation for reference data and owned.
  Object(Vec<(&'ctx str, Value<'ctx>)>),
  ObjectOwned(JsonObject),
  // Fk
  ForeignKey {
    id: serde_json::Value,
    data: Option<Box<serde_json::value::RawValue>>,
  },
  // Nested unparsed json.
  Raw(Box<serde_json::value::RawValue>),
}

impl<'ctx> From<Value<'ctx>> for serde_json::Value {
  fn from(value: Value<'ctx>) -> Self {
    use serde_json::Value as JValue;
    return match value {
      Value::Null => JValue::Null,
      Value::Bool(b) => JValue::Bool(b),
      Value::Number(n) => JValue::Number(n),
      Value::String(s) => JValue::String(s.to_string()),
      Value::Array(a) => JValue::Array(a.into_iter().map(|v| v.into()).collect()),
      Value::Object(o) => JValue::Object(
        o.into_iter()
          .map(|(k, v)| (k.to_string(), v.into()))
          .collect(),
      ),
      Value::ObjectOwned(o) => JValue::Object(o),
      Value::ForeignKey { id, data } => {
        if let Some(data) = data {
          serde_json::json!({
            "id": id,
            "data": serde_json::from_str::<JValue>(data.get()).expect("well-formed"),
          })
        } else {
          serde_json::json!({
            "id": id,
          })
        }
      }
      Value::Raw(raw) => serde_json::from_str(raw.get()).expect("well-formed"),
    };
  }
}

impl serde::ser::Serialize for Value<'_> {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::ser::Serializer,
  {
    match self {
      Value::Null => serializer.serialize_unit(),
      Value::Bool(b) => serializer.serialize_bool(*b),
      Value::Number(n) => n.serialize(serializer),
      Value::String(s) => serializer.serialize_str(s),
      Value::Array(v) => serializer.collect_seq(v),
      Value::Object(m) => serializer.collect_map(m.iter().map(|(k, v)| (*k, v))),
      Value::ObjectOwned(m) => m.serialize(serializer),
      Value::ForeignKey { id, data } => {
        use serde::ser::SerializeMap;
        let mut s = serializer.serialize_map(Some(if data.is_some() { 2 } else { 1 }))?;
        s.serialize_entry("id", id)?;
        if let Some(data) = data {
          s.serialize_entry("data", data)?;
        }
        s.end()
      }
      Value::Raw(j) => j.serialize(serializer),
    }
  }
}
