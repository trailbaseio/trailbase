use base64::prelude::*;
use serde::de::{Deserialize, Deserializer, Error, Unexpected};
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
  // Note that bytes are also strings, either UUID or url-safe-b64 encoded. Need to be decoded
  // downstream based on content.
  String(String),
  Integer(i64),
  Double(f64),
  Bool(bool),
}

impl Value {
  fn unparse(value: String) -> Self {
    return match value.as_str() {
      "true" | "TRUE" => Value::Bool(true),
      "false" | "FALSE" => Value::Bool(false),
      _ => {
        if let Ok(i) = i64::from_str(&value) {
          Value::Integer(i)
        } else if let Ok(d) = f64::from_str(&value) {
          Value::Double(d)
        } else {
          Value::String(value)
        }
      }
    };
  }

  pub fn to_sql(&self) -> String {
    return match self {
      Self::String(s) => format!("'{s}'"),
      Self::Integer(i) => i.to_string(),
      Self::Double(d) => d.to_string(),
      Self::Bool(b) => match b {
        true => "TRUE".to_string(),
        false => "false".to_string(),
      },
    };
  }
}

impl std::fmt::Display for Value {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    return match self {
      Self::String(s) => s.fmt(f),
      Self::Integer(i) => i.fmt(f),
      Self::Double(d) => d.fmt(f),
      Self::Bool(b) => b.fmt(f),
    };
  }
}

pub fn serde_value_to_value<'de, D>(value: serde_value::Value) -> Result<Value, D::Error>
where
  D: Deserializer<'de>,
{
  return match value {
    serde_value::Value::String(value) => Ok(Value::unparse(value)),
    serde_value::Value::Bytes(bytes) => Ok(Value::String(BASE64_URL_SAFE.encode(bytes))),
    serde_value::Value::I64(i) => Ok(Value::Integer(i)),
    serde_value::Value::I32(i) => Ok(Value::Integer(i as i64)),
    serde_value::Value::I16(i) => Ok(Value::Integer(i as i64)),
    serde_value::Value::I8(i) => Ok(Value::Integer(i as i64)),
    serde_value::Value::U64(i) => Ok(Value::Integer(i as i64)),
    serde_value::Value::U32(i) => Ok(Value::Integer(i as i64)),
    serde_value::Value::U16(i) => Ok(Value::Integer(i as i64)),
    serde_value::Value::U8(i) => Ok(Value::Integer(i as i64)),
    serde_value::Value::Bool(b) => Ok(Value::Bool(b)),
    _ => Err(Error::invalid_type(unexpected(&value), &"Value")),
  };
}

impl<'de> Deserialize<'de> for Value {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    use serde_value::Value;
    let value = Value::deserialize(deserializer)?;

    return serde_value_to_value::<'de, D>(value);
  }
}

pub fn unexpected(value: &serde_value::Value) -> Unexpected {
  use serde_value::Value;
  match *value {
    Value::Bool(b) => serde::de::Unexpected::Bool(b),
    Value::U8(n) => serde::de::Unexpected::Unsigned(n as u64),
    Value::U16(n) => serde::de::Unexpected::Unsigned(n as u64),
    Value::U32(n) => serde::de::Unexpected::Unsigned(n as u64),
    Value::U64(n) => serde::de::Unexpected::Unsigned(n),
    Value::I8(n) => serde::de::Unexpected::Signed(n as i64),
    Value::I16(n) => serde::de::Unexpected::Signed(n as i64),
    Value::I32(n) => serde::de::Unexpected::Signed(n as i64),
    Value::I64(n) => serde::de::Unexpected::Signed(n),
    Value::F32(n) => serde::de::Unexpected::Float(n as f64),
    Value::F64(n) => serde::de::Unexpected::Float(n),
    Value::Char(c) => serde::de::Unexpected::Char(c),
    Value::String(ref s) => serde::de::Unexpected::Str(s),
    Value::Unit => serde::de::Unexpected::Unit,
    Value::Option(_) => serde::de::Unexpected::Option,
    Value::Newtype(_) => serde::de::Unexpected::NewtypeStruct,
    Value::Seq(_) => serde::de::Unexpected::Seq,
    Value::Map(_) => serde::de::Unexpected::Map,
    Value::Bytes(ref b) => serde::de::Unexpected::Bytes(b),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use serde::Deserialize;
  use serde_qs::Config;

  use crate::column_rel_value::{ColumnOpValue, CompareOp};
  use crate::filter::ValueOrComposite;

  #[derive(Clone, Debug, Default, Deserialize)]
  struct Query {
    filter: Option<ValueOrComposite>,
  }

  #[test]
  fn test_value() {
    let qs = Config::new(5, true);

    let v0: Query = qs.deserialize_str("filter[col0][eq]=val0").unwrap();
    assert_eq!(
      v0.filter.unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "col0".to_string(),
        op: CompareOp::Equal,
        value: Value::String("val0".to_string()),
      })
    );
    let v1: Query = qs.deserialize_str("filter[col0][ne]=TRUE").unwrap();
    assert_eq!(
      v1.filter.unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "col0".to_string(),
        op: CompareOp::NotEqual,
        value: Value::Bool(true),
      })
    );

    let v2: Query = qs.deserialize_str("filter[col0][ne]=0").unwrap();
    assert_eq!(
      v2.filter.unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "col0".to_string(),
        op: CompareOp::NotEqual,
        value: Value::Integer(0),
      })
    );

    let v3: Query = qs.deserialize_str("filter[col0][ne]=0.0").unwrap();
    assert_eq!(
      v3.filter.unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "col0".to_string(),
        op: CompareOp::NotEqual,
        value: Value::Double(0.0),
      })
    );
  }
}
