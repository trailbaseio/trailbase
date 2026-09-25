use crate::from_sql::FromSqlError;

pub type Blob = Vec<u8>;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
  /// The value is a `NULL` value.
  #[default]
  Null,
  /// The value is a signed integer.
  Integer(i64),
  /// The value is a floating point number.
  Real(f64),
  /// The value is a text string.
  Text(String),
  /// The value is a blob of data
  Blob(Blob),
}

impl From<bool> for Value {
  #[inline]
  fn from(v: bool) -> Self {
    return Self::Integer(if v { 1 } else { 0 });
  }
}

impl From<i64> for Value {
  #[inline]
  fn from(i: i64) -> Self {
    return Self::Integer(i);
  }
}

impl From<f32> for Value {
  #[inline]
  fn from(f: f32) -> Self {
    return Self::Real(f.into());
  }
}

impl From<f64> for Value {
  #[inline]
  fn from(f: f64) -> Self {
    return Self::Real(f);
  }
}

impl From<String> for Value {
  #[inline]
  fn from(s: String) -> Self {
    return Self::Text(s);
  }
}

impl From<Vec<u8>> for Value {
  #[inline]
  fn from(v: Vec<u8>) -> Self {
    return Self::Blob(v);
  }
}

impl From<&[u8]> for Value {
  #[inline]
  fn from(v: &[u8]) -> Self {
    return Self::Blob(v.to_vec());
  }
}

impl<const N: usize> From<&[u8; N]> for Value {
  #[inline]
  fn from(v: &[u8; N]) -> Self {
    return Self::Blob(v.into());
  }
}

impl TryFrom<Value> for String {
  type Error = FromSqlError;

  fn try_from(value: Value) -> Result<Self, Self::Error> {
    return match value {
      Value::Text(s) => Ok(s),
      _ => Err(Self::Error::InvalidType),
    };
  }
}

impl<T: TryFrom<Value, Error = FromSqlError>> TryFrom<Value> for Option<T> {
  type Error = FromSqlError;

  fn try_from(value: Value) -> Result<Self, Self::Error> {
    return match value {
      Value::Null => Ok(None),
      v => Ok(Some(v.try_into()?)),
    };
  }
}
