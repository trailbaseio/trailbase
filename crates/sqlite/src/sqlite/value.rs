use crate::value::Value;
use crate::value_ref::ValueRef;

// Convert between two value types.
impl From<Value> for rusqlite::types::Value {
  fn from(value: Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as RusqliteValue;

    return match value {
      Value::Null => RusqliteValue::Null,
      Value::Integer(i) => RusqliteValue::Integer(i),
      Value::Real(f) => RusqliteValue::Real(f),
      Value::Text(t) => RusqliteValue::Text(t),
      Value::Blob(b) => RusqliteValue::Blob(b.to_vec()),
    };
  }
}

// Convert &Value to rusqlite::types::ValueRef
impl<'a> From<&'a Value> for rusqlite::types::ValueRef<'a> {
  #[inline]
  fn from(value: &'a Value) -> Self {
    use rusqlite::types::ValueRef as SqliteValueRef;

    return match *value {
      Value::Null => SqliteValueRef::Null,
      Value::Integer(i) => SqliteValueRef::Integer(i),
      Value::Real(r) => SqliteValueRef::Real(r),
      Value::Text(ref s) => SqliteValueRef::Text(s.as_bytes()),
      Value::Blob(ref b) => SqliteValueRef::Blob(b),
    };
  }
}

impl TryFrom<rusqlite::types::ValueRef<'_>> for Value {
  type Error = rusqlite::types::FromSqlError;

  #[inline]
  fn try_from(borrowed: rusqlite::types::ValueRef<'_>) -> Result<Self, Self::Error> {
    return match borrowed {
      rusqlite::types::ValueRef::Null => Ok(Self::Null),
      rusqlite::types::ValueRef::Integer(i) => Ok(Self::Integer(i)),
      rusqlite::types::ValueRef::Real(r) => Ok(Self::Real(r)),
      rusqlite::types::ValueRef::Text(s) => Ok(Self::Text(
        std::str::from_utf8(s)
          .map_err(Self::Error::Utf8Error)?
          .to_owned(),
      )),
      rusqlite::types::ValueRef::Blob(b) => Ok(Self::Blob(b.to_vec())),
    };
  }
}

impl rusqlite::types::FromSql for Value {
  fn column_result(
    value: rusqlite::types::ValueRef<'_>,
  ) -> Result<Self, rusqlite::types::FromSqlError> {
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

impl<'a> From<rusqlite::types::ValueRef<'a>> for ValueRef<'a> {
  fn from(value: rusqlite::types::ValueRef<'a>) -> ValueRef<'a> {
    use rusqlite::types::ValueRef as SqliteValueRef;

    return match value {
      SqliteValueRef::Null => ValueRef::Null,
      SqliteValueRef::Integer(i) => ValueRef::Integer(i),
      SqliteValueRef::Real(f) => ValueRef::Real(f),
      SqliteValueRef::Text(t) => ValueRef::Text(t),
      SqliteValueRef::Blob(b) => ValueRef::Blob(b),
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
