use serde::de::{Deserializer, Error};

use crate::value::Value;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompareOp {
  Equal,
  NotEqual,
  GreaterThanEqual,
  GreaterThan,
  LessThanEqual,
  LessThan,
  Null,
  NotNull,
  Like,
  Regexp,
}

impl CompareOp {
  pub fn from(qualifier: &str) -> Option<Self> {
    return match qualifier {
      "$eq" => Some(Self::Equal),
      "$ne" => Some(Self::NotEqual),
      "$gte" => Some(Self::GreaterThanEqual),
      "$gt" => Some(Self::GreaterThan),
      "$lte" => Some(Self::LessThanEqual),
      "$lt" => Some(Self::LessThan),
      "$null" => Some(Self::Null),
      "$some" => Some(Self::NotNull),
      "$like" => Some(Self::Like),
      "$re" => Some(Self::Regexp),
      _ => None,
    };
  }

  pub fn is_unary(&self) -> bool {
    return matches!(self, Self::Null | Self::NotNull);
  }

  pub fn to_sql(self) -> &'static str {
    return match self {
      Self::GreaterThanEqual => ">=",
      Self::GreaterThan => ">",
      Self::LessThanEqual => "<=",
      Self::LessThan => "<",
      Self::NotEqual => "<>",
      Self::Null => "IS NULL",
      Self::NotNull => "IS NOT NULL",
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

    static EXPECTED: &str = "one of [$eq, $ne, $lt, ...]";

    let value = Value::deserialize(deserializer)?;
    let Value::String(ref string) = value else {
      return Err(Error::invalid_type(
        crate::util::unexpected(&value),
        &EXPECTED,
      ));
    };

    return CompareOp::from(string)
      .ok_or_else(|| Error::invalid_type(crate::util::unexpected(&value), &EXPECTED));
  }
}

/// Type to support query of shape: `[column][op]=value`.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnOpValue {
  pub column: String,
  pub op: CompareOp,
  pub value: Value,
}

pub fn serde_value_to_single_column_rel_value<'de, D>(
  key: String,
  value: serde_value::Value,
) -> Result<ColumnOpValue, D::Error>
where
  D: Deserializer<'de>,
{
  use serde_value::Value;
  if !crate::util::sanitize_column_name(&key) {
    // NOTE: This may trigger if serde_qs parse depth is not enough. In this case, square brakets
    // will end up in the column name.
    return Err(Error::custom(format!(
      "invalid column name for filter: {key}. Nesting too deep?"
    )));
  }

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
      crate::util::unexpected(&v),
      &"[column_name]=value or [column_name][$op]=value",
    )),
  };
}
