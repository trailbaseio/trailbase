#[derive(Clone, Debug, PartialEq)]
pub enum Value {
  /// The value is a `NULL` value.
  Null,
  /// The value is a signed integer.
  Integer(i64),
  /// The value is a floating point number.
  Real(f64),
  /// The value is a text string.
  Text(String),
  /// The value is a blob of data
  Blob(Vec<u8>),
}

impl From<bool> for Value {
  #[inline]
  fn from(v: bool) -> Self {
    Self::Integer(if v { 1 } else { 0 })
  }
}

impl From<i64> for Value {
  #[inline]
  fn from(i: i64) -> Self {
    Self::Integer(i)
  }
}

impl From<f32> for Value {
  #[inline]
  fn from(f: f32) -> Self {
    Self::Real(f.into())
  }
}

impl From<f64> for Value {
  #[inline]
  fn from(f: f64) -> Self {
    Self::Real(f)
  }
}

impl From<String> for Value {
  #[inline]
  fn from(s: String) -> Self {
    Self::Text(s)
  }
}

impl From<Vec<u8>> for Value {
  #[inline]
  fn from(v: Vec<u8>) -> Self {
    Self::Blob(v)
  }
}

// Convert between two value types.
impl From<Value> for rusqlite::types::Value {
  fn from(value: Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as SqliteValue;

    return match value {
      Value::Null => SqliteValue::Null,
      Value::Integer(i) => SqliteValue::Integer(i),
      Value::Real(f) => SqliteValue::Real(f),
      Value::Text(t) => SqliteValue::Text(t),
      Value::Blob(b) => SqliteValue::Blob(b),
    };
  }
}

// Convert &Value to rusqlite::types::ValueRef
impl<'a> From<&'a Value> for rusqlite::types::ValueRef<'a> {
  #[inline]
  fn from(value: &'a Value) -> Self {
    use rusqlite::types::ValueRef as SqliteValueRef;

    match *value {
      Value::Null => SqliteValueRef::Null,
      Value::Integer(i) => SqliteValueRef::Integer(i),
      Value::Real(r) => SqliteValueRef::Real(r),
      Value::Text(ref s) => SqliteValueRef::Text(s.as_bytes()),
      Value::Blob(ref b) => SqliteValueRef::Blob(b),
    }
  }
}

impl TryFrom<rusqlite::types::ValueRef<'_>> for Value {
  type Error = rusqlite::types::FromSqlError;

  #[inline]
  fn try_from(borrowed: rusqlite::types::ValueRef<'_>) -> Result<Self, Self::Error> {
    match borrowed {
      rusqlite::types::ValueRef::Null => Ok(Self::Null),
      rusqlite::types::ValueRef::Integer(i) => Ok(Self::Integer(i)),
      rusqlite::types::ValueRef::Real(r) => Ok(Self::Real(r)),
      rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s)
        .map(|s| Self::Text(s.to_string()))
        .map_err(Self::Error::Utf8Error),
      rusqlite::types::ValueRef::Blob(b) => Ok(Self::Blob(b.to_vec())),
    }
  }
}

impl rusqlite::types::FromSql for Value {
  fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
    return value.try_into();
  }
}

impl rusqlite::types::ToSql for Value {
  fn to_sql(&self) -> Result<rusqlite::types::ToSqlOutput<'_>, rusqlite::Error> {
    return Ok(rusqlite::types::ToSqlOutput::Borrowed(
      rusqlite::types::ValueRef::from(self),
    ));
  }
}

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

impl<'a> From<ValueRef<'a>> for rusqlite::types::ValueRef<'a> {
  fn from(value: ValueRef<'a>) -> rusqlite::types::ValueRef<'a> {
    use rusqlite::types::ValueRef as SqliteValueRef;

    return match value {
      ValueRef::Null => SqliteValueRef::Null,
      ValueRef::Integer(i) => SqliteValueRef::Integer(i),
      ValueRef::Real(f) => SqliteValueRef::Real(f),
      ValueRef::Text(t) => SqliteValueRef::Text(t),
      ValueRef::Blob(b) => SqliteValueRef::Blob(b),
    };
  }
}

impl<'a> rusqlite::types::ToSql for ValueRef<'a> {
  fn to_sql(&self) -> Result<rusqlite::types::ToSqlOutput<'_>, rusqlite::Error> {
    return Ok(rusqlite::types::ToSqlOutput::Borrowed(
      rusqlite::types::ValueRef::from(*self),
    ));
  }
}

impl<'a> From<&'a str> for ValueRef<'a> {
  #[inline]
  fn from(s: &str) -> ValueRef<'_> {
    ValueRef::Text(s.as_bytes())
  }
}

impl<'a> From<&'a [u8]> for ValueRef<'a> {
  #[inline]
  fn from(s: &[u8]) -> ValueRef<'_> {
    ValueRef::Blob(s)
  }
}

impl<'a> From<&'a Value> for ValueRef<'a> {
  #[inline]
  fn from(value: &'a Value) -> Self {
    match *value {
      Value::Null => ValueRef::Null,
      Value::Integer(i) => ValueRef::Integer(i),
      Value::Real(i) => ValueRef::Real(i),
      Value::Text(ref s) => ValueRef::Text(s.as_bytes()),
      Value::Blob(ref b) => ValueRef::Blob(b),
    }
  }
}

impl<T> From<Option<T>> for ValueRef<'_>
where
  T: Into<Self>,
{
  #[inline]
  fn from(s: Option<T>) -> Self {
    match s {
      Some(x) => x.into(),
      None => ValueRef::Null,
    }
  }
}
