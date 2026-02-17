use base64::prelude::*;
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
  Is,
  Like,
  Regexp,

  // Spatial Types:
  StWithin,
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
      "$is" => Some(Self::Is),
      "$like" => Some(Self::Like),
      "$re" => Some(Self::Regexp),
      "@within" => Some(Self::StWithin),
      _ => None,
    };
  }

  #[inline]
  pub fn as_sql(&self, column: &str, param: &str) -> String {
    return match self {
      Self::GreaterThanEqual => format!("{column} >= {param}"),
      Self::GreaterThan => format!("{column} > {param}"),
      Self::LessThanEqual => format!("{column} <= {param}"),
      Self::LessThan => format!("{column} < {param}"),
      Self::NotEqual => format!("{column} <> {param}"),
      Self::Is => format!("{column} IS {param}"),
      Self::Like => format!("{column} LIKE {param}"),
      Self::Regexp => format!("{column} REGEXP {param}"),
      Self::Equal => format!("{column} = {param}"),
      Self::StWithin => format!("ST_Within({column}, {param})"),
    };
  }

  #[inline]
  pub fn as_query(&self) -> &'static str {
    return match self {
      Self::Equal => "$eq",
      Self::NotEqual => "$ne",
      Self::GreaterThanEqual => "$gte",
      Self::GreaterThan => "$gt",
      Self::LessThanEqual => "$lte",
      Self::LessThan => "$lt",
      Self::Is => "$is",
      Self::Like => "$like",
      Self::Regexp => "$re",
      Self::StWithin => "@within",
    };
  }
}

/// Type to support query of shape: `[column][op]=value`.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnOpValue {
  pub column: String,
  pub op: CompareOp,
  pub value: Value,
}

fn parse_value<'de, D>(op: CompareOp, value: serde_value::Value) -> Result<Value, D::Error>
where
  D: Deserializer<'de>,
{
  use crate::util::unexpected;

  return match op {
    CompareOp::Is => match value {
      serde_value::Value::String(value) if value == "NULL" => Ok(Value::String("NULL".to_string())),
      serde_value::Value::String(value) if value == "!NULL" => {
        Ok(Value::String("NOT NULL".to_string()))
      }
      _ => Err(Error::invalid_type(unexpected(&value), &"NULL or !NULL")),
    },
    CompareOp::StWithin => {
      use wkt::TryFromWkt;
      match value {
        serde_value::Value::String(v)
          if geo_types::Geometry::<f64>::try_from_wkt_str(&v).is_ok() =>
        {
          Ok(Value::String(v))
        }
        _ => Err(Error::invalid_type(unexpected(&value), &"WKT Geometry")),
      }
    }
    _ => match value {
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
      serde_value::Value::Bool(b) => Ok(Value::Integer(if b { 1 } else { 0 })),
      _ => Err(Error::invalid_type(
        unexpected(&value),
        &"trailbase_qs::Value, i.e. string, integer, double or bool",
      )),
    },
  };
}

pub fn serde_value_to_single_column_rel_value<'de, D>(
  key: String,
  value: serde_value::Value,
) -> Result<ColumnOpValue, D::Error>
where
  D: Deserializer<'de>,
{
  use crate::util::unexpected;
  use serde_value::Value;

  if !crate::util::sanitize_column_name(&key) {
    // NOTE: This may trigger if serde_qs parse depth is not enough. In this case, square brackets
    // will end up in the column name.
    return Err(Error::custom(format!(
      "invalid column name for filter: {key}. Nesting too deep?"
    )));
  }

  return match value {
    // The simple ?filter[col]=val case.
    Value::String(_) => Ok(ColumnOpValue {
      column: key,
      op: CompareOp::Equal,
      value: parse_value::<D>(CompareOp::Equal, value)?,
    }),
    // The operator case ?filter[col][$ne]=val case.
    Value::Map(mut m) if m.len() == 1 => {
      let (k, v) = m.pop_first().expect("len() == 1");

      let op = if let Value::String(ref op_str) = k {
        CompareOp::from(op_str).ok_or_else(|| Error::invalid_type(unexpected(&k), &OP_ERR))?
      } else {
        return Err(Error::invalid_type(unexpected(&k), &OP_ERR));
      };

      Ok(ColumnOpValue {
        column: key,
        value: parse_value::<D>(op, v)?,
        op,
      })
    }
    v => Err(Error::invalid_type(
      unexpected(&v),
      &"[column_name]=value or [column_name][$op]=value",
    )),
  };
}

const OP_ERR: &str = "one of [$eq, $ne, $lt, ...]";
