use serde::de::{Deserializer, Error};

use crate::value::{Value, unexpected};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompareOp {
  Not,
  Equal,
  NotEqual,
  GreaterThanEqual,
  GreaterThan,
  LessThanEqual,
  LessThan,
  Like,
  Regexp,
}

impl CompareOp {
  pub fn from(qualifier: &str) -> Option<Self> {
    return match qualifier {
      "gte" => Some(Self::GreaterThanEqual),
      "gt" => Some(Self::GreaterThan),
      "lte" => Some(Self::LessThanEqual),
      "lt" => Some(Self::LessThan),
      "eq" => Some(Self::Equal),
      "not" => Some(Self::Not),
      "ne" => Some(Self::NotEqual),
      "like" => Some(Self::Like),
      "re" => Some(Self::Regexp),
      _ => None,
    };
  }

  pub fn to_sql(self) -> &'static str {
    return match self {
      Self::GreaterThanEqual => ">=",
      Self::GreaterThan => ">",
      Self::LessThanEqual => "<=",
      Self::LessThan => "<",
      Self::Not => "<>",
      Self::NotEqual => "<>",
      Self::Like => "LIKE",
      Self::Regexp => "REGEXP",
      Self::Equal => "=",
    };
  }
}

impl<'de> serde::de::Deserialize<'de> for CompareOp {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    use serde_value::Value;
    let value = Value::deserialize(deserializer)?;

    let Value::String(ref string) = value else {
      return Err(Error::invalid_type(unexpected(&value), &"String"));
    };

    return CompareOp::from(string)
      .ok_or_else(|| Error::invalid_type(unexpected(&value), &"(eq|ne)"));
  }
}

/// Type to support query of shape: `[column][op]=value`.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnOpValue {
  pub column: String,
  pub op: CompareOp,
  pub value: Value,
}

impl ColumnOpValue {
  pub fn to_sql(&self) -> String {
    return format!(
      "{c} {o} {v}",
      c = self.column,
      o = self.op.to_sql(),
      v = self.value.to_sql()
    );
  }
}

pub fn serde_value_to_single_column_rel_value<'de, D>(
  key: String,
  value: serde_value::Value,
) -> Result<ColumnOpValue, D::Error>
where
  D: Deserializer<'de>,
{
  use serde_value::Value;

  return match value {
    Value::String(_) => Ok(ColumnOpValue {
      column: key,
      op: CompareOp::Equal,
      value: crate::value::serde_value_to_value::<D>(value)?,
    }),
    Value::Map(mut m) if m.len() == 1 => {
      let (k, v) = m.pop_first().expect("len() == 1");

      let op = k.deserialize_into::<CompareOp>().map_err(Error::custom)?;

      Ok(ColumnOpValue {
        column: key,
        op,
        value: crate::value::serde_value_to_value::<D>(v)?,
      })
    }
    v => Err(Error::invalid_type(
      unexpected(&v),
      &"Map<String, String | Map<Rel, String>>",
    )),
  };
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColumnOpValueMap(pub(crate) Vec<ColumnOpValue>);

impl<'de> serde::de::Deserialize<'de> for ColumnOpValueMap {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    use serde_value::Value;
    let value = Value::deserialize(deserializer)?;

    let Value::Map(m) = value else {
      return Err(Error::invalid_type(
        unexpected(&value),
        &"Map<String, String | Map<Rel, String>>",
      ));
    };

    let vec = m
      .into_iter()
      .map(|(k, v)| {
        return match (k, v) {
          (Value::String(key), v) => serde_value_to_single_column_rel_value::<D>(key, v),
          (k, _) => Err(Error::invalid_type(unexpected(&k), &"String")),
        };
      })
      .collect::<Result<Vec<_>, _>>()?;

    return Ok(ColumnOpValueMap(vec));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use serde::Deserialize;
  use serde_qs::Config;

  #[derive(Clone, Debug, Deserialize)]
  struct Query {
    filter: Option<ColumnOpValueMap>,
  }

  #[test]
  fn test_column_rel_value() {
    let qs = Config::new(5, true);

    let m_empty: Query = qs.deserialize_str("").unwrap();
    assert_eq!(m_empty.filter, None);

    let m0: Query = qs.deserialize_str("filter[column]=1").unwrap();
    assert_eq!(
      m0.filter.unwrap().0,
      vec![ColumnOpValue {
        column: "column".to_string(),
        op: CompareOp::Equal,
        value: Value::Integer(1),
      }]
    );

    let m0: Query = qs.deserialize_str("filter[column]=true").unwrap();
    assert_eq!(
      m0.filter.unwrap().0,
      vec![ColumnOpValue {
        column: "column".to_string(),
        op: CompareOp::Equal,
        value: Value::Bool(true),
      }]
    );

    let m1: Query = qs.deserialize_str("filter[column][eq]=1").unwrap();
    assert_eq!(
      m1.filter.unwrap().0,
      vec![ColumnOpValue {
        column: "column".to_string(),
        op: CompareOp::Equal,
        value: Value::Integer(1),
      }]
    );

    let m2: Query = qs
      .deserialize_str("filter[col1][ne]=1&filter[col2]=2")
      .unwrap();
    assert_eq!(
      m2.filter.unwrap().0,
      vec![
        ColumnOpValue {
          column: "col1".to_string(),
          op: CompareOp::NotEqual,
          value: Value::Integer(1),
        },
        ColumnOpValue {
          column: "col2".to_string(),
          op: CompareOp::Equal,
          value: Value::Integer(2),
        },
      ]
    );
  }
}
