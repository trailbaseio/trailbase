use std::str::FromStr;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
  // Note that bytes are also strings, either UUID or url-safe-b64 encoded. Need to be decoded
  // downstream based on content.
  String(String),
  Integer(i64),
  Double(f64),
}

impl Value {
  pub(crate) fn unparse(value: String) -> Self {
    return if let Ok(i) = i64::from_str(&value) {
      Value::Integer(i)
    } else if let Ok(d) = f64::from_str(&value) {
      Value::Double(d)
    } else {
      Value::String(value)
    };
  }
}

impl std::fmt::Display for Value {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    return match self {
      Self::String(s) => s.fmt(f),
      Self::Integer(i) => i.fmt(f),
      Self::Double(d) => d.fmt(f),
    };
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
    let qs = Config::new();

    assert_eq!(
      qs.deserialize_str::<Query>("filter[col0][$eq]=val0")
        .unwrap()
        .filter
        .unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "col0".to_string(),
        op: CompareOp::Equal,
        value: Value::String("val0".to_string()),
      })
    );

    assert_eq!(
      qs.deserialize_str::<Query>("filter[col0][$ne]=TRUE")
        .unwrap()
        .filter
        .unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "col0".to_string(),
        op: CompareOp::NotEqual,
        value: Value::String("TRUE".to_string()),
      })
    );

    assert_eq!(
      qs.deserialize_str::<Query>("filter[col0][$ne]=0")
        .unwrap()
        .filter
        .unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "col0".to_string(),
        op: CompareOp::NotEqual,
        value: Value::Integer(0),
      })
    );

    assert_eq!(
      qs.deserialize_str::<Query>("filter[col0][$ne]=0.0")
        .unwrap()
        .filter
        .unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "col0".to_string(),
        op: CompareOp::NotEqual,
        value: Value::Double(0.0),
      })
    );

    assert_eq!(
      qs.deserialize_str::<Query>("filter[col0][$is]=NULL")
        .unwrap()
        .filter
        .unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "col0".to_string(),
        op: CompareOp::Is,
        value: Value::String("NULL".to_string()),
      })
    );
  }
}
