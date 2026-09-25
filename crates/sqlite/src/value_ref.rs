use crate::from_sql::{FromSqlError, FromSqlResult};
use crate::value::Value;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ValueRef<'a> {
  /// The value is a `NULL` value.
  Null,
  /// The value is a signed integer.
  Integer(i64),
  /// The value is a floating point number.
  Real(f64),
  /// The value is a text string.
  Text(&'a [u8]),
  /// The value is a blob of data
  Blob(&'a [u8]),
}

impl<'a> ValueRef<'a> {
  #[inline]
  pub fn as_i64(&self) -> FromSqlResult<i64> {
    return match *self {
      ValueRef::Integer(i) => Ok(i),
      _ => Err(FromSqlError::InvalidType),
    };
  }

  #[inline]
  pub fn as_i64_or_null(&self) -> FromSqlResult<Option<i64>> {
    return match *self {
      ValueRef::Null => Ok(None),
      ValueRef::Integer(i) => Ok(Some(i)),
      _ => Err(FromSqlError::InvalidType),
    };
  }

  #[inline]
  pub fn as_f64(&self) -> FromSqlResult<f64> {
    return match *self {
      ValueRef::Real(f) => Ok(f),
      _ => Err(FromSqlError::InvalidType),
    };
  }

  #[inline]
  pub fn as_f64_or_null(&self) -> FromSqlResult<Option<f64>> {
    return match *self {
      ValueRef::Null => Ok(None),
      ValueRef::Real(f) => Ok(Some(f)),
      _ => Err(FromSqlError::InvalidType),
    };
  }

  #[inline]
  pub fn as_str(&self) -> FromSqlResult<&'a str> {
    return match *self {
      ValueRef::Text(t) => std::str::from_utf8(t).map_err(FromSqlError::Utf8Error),
      _ => Err(FromSqlError::InvalidType),
    };
  }

  #[inline]
  pub fn as_str_or_null(&self) -> FromSqlResult<Option<&'a str>> {
    return match *self {
      ValueRef::Null => Ok(None),
      ValueRef::Text(t) => std::str::from_utf8(t)
        .map_err(FromSqlError::Utf8Error)
        .map(Some),
      _ => Err(FromSqlError::InvalidType),
    };
  }

  #[inline]
  pub fn as_blob(&self) -> FromSqlResult<&'a [u8]> {
    return match *self {
      ValueRef::Blob(b) => Ok(b),
      _ => Err(FromSqlError::InvalidType),
    };
  }

  #[inline]
  pub fn as_blob_or_null(&self) -> FromSqlResult<Option<&'a [u8]>> {
    return match *self {
      ValueRef::Null => Ok(None),
      ValueRef::Blob(b) => Ok(Some(b)),
      _ => Err(FromSqlError::InvalidType),
    };
  }

  #[inline]
  pub fn as_bytes(&self) -> FromSqlResult<&'a [u8]> {
    return match self {
      ValueRef::Text(s) | ValueRef::Blob(s) => Ok(s),
      _ => Err(FromSqlError::InvalidType),
    };
  }

  #[inline]
  pub fn as_bytes_or_null(&self) -> FromSqlResult<Option<&'a [u8]>> {
    return match *self {
      ValueRef::Null => Ok(None),
      ValueRef::Text(s) | ValueRef::Blob(s) => Ok(Some(s)),
      _ => Err(FromSqlError::InvalidType),
    };
  }
}

impl<'a> From<&'a str> for ValueRef<'a> {
  #[inline]
  fn from(s: &str) -> ValueRef<'_> {
    return ValueRef::Text(s.as_bytes());
  }
}

impl<'a> From<&'a [u8]> for ValueRef<'a> {
  #[inline]
  fn from(s: &[u8]) -> ValueRef<'_> {
    return ValueRef::Blob(s);
  }
}

impl<'a> From<&'a Value> for ValueRef<'a> {
  #[inline]
  fn from(value: &'a Value) -> Self {
    return match *value {
      Value::Null => ValueRef::Null,
      Value::Integer(i) => ValueRef::Integer(i),
      Value::Real(i) => ValueRef::Real(i),
      Value::Text(ref s) => ValueRef::Text(s.as_bytes()),
      Value::Blob(ref b) => ValueRef::Blob(b),
    };
  }
}

impl<T> From<Option<T>> for ValueRef<'_>
where
  T: Into<Self>,
{
  #[inline]
  fn from(s: Option<T>) -> Self {
    return match s {
      Some(x) => x.into(),
      _ => ValueRef::Null,
    };
  }
}

impl TryFrom<ValueRef<'_>> for Value {
  type Error = crate::from_sql::FromSqlError;

  #[inline]
  fn try_from(borrowed: ValueRef<'_>) -> Result<Self, Self::Error> {
    return match borrowed {
      ValueRef::Null => Ok(Self::Null),
      ValueRef::Integer(i) => Ok(Self::Integer(i)),
      ValueRef::Real(r) => Ok(Self::Real(r)),
      ValueRef::Text(s) => std::str::from_utf8(s)
        .map(|s| Self::Text(s.to_string()))
        .map_err(Self::Error::Utf8Error),
      ValueRef::Blob(b) => Ok(Self::Blob(b.to_vec())),
    };
  }
}
