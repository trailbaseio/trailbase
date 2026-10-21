use compact_str::CompactString;

use crate::value::Value;
use crate::value_ref::ValueRef;

// Proxy/strong-typedef that only exists to implement `params!`/`named_params!`.
#[allow(missing_debug_implementations)]
pub enum ToSqlProxy<'a> {
  /// A borrowed SQLite-representable value.
  Borrowed(ValueRef<'a>),

  /// An owned SQLite-representable value.
  Owned(Value),
}

impl<'a, T: ?Sized> From<&'a T> for ToSqlProxy<'a>
where
  &'a T: Into<ValueRef<'a>>,
{
  #[inline]
  fn from(t: &'a T) -> Self {
    ToSqlProxy::Borrowed(t.into())
  }
}

macro_rules! from_value(
    ($t:ty) => (
        impl From<$t> for ToSqlProxy<'_> {
            #[inline]
            fn from(t: $t) -> Self { ToSqlProxy::Owned(t.into())}
        }
    )
);

from_value!(String);
from_value!(bool);
from_value!(i64);
from_value!(f64);
from_value!(Vec<u8>);
from_value!(Value);

impl<'a, const N: usize> From<[u8; N]> for ToSqlProxy<'a> {
  fn from(bytes: [u8; N]) -> Self {
    return ToSqlProxy::Owned(Value::Blob(bytes.to_vec()));
  }
}

impl<'a, const N: usize> From<&'a [u8; N]> for ToSqlProxy<'a> {
  fn from(bytes: &'a [u8; N]) -> Self {
    ToSqlProxy::Borrowed(ValueRef::Blob(bytes))
  }
}

impl<'a> From<CompactString> for ToSqlProxy<'a> {
  fn from(s: CompactString) -> Self {
    ToSqlProxy::Owned(s.into())
  }
}

impl<'a, T> From<Option<T>> for ToSqlProxy<'a>
where
  T: Into<ToSqlProxy<'a>>,
{
  fn from(t: Option<T>) -> Self {
    match t {
      Some(t) => t.into(),
      None => ToSqlProxy::Borrowed(ValueRef::Null),
    }
  }
}

impl<'a> TryFrom<ToSqlProxy<'a>> for Value {
  // QUESTION: Should this be a FromSqlError?
  type Error = crate::Error;

  fn try_from(value: ToSqlProxy<'a>) -> Result<Self, Self::Error> {
    match value {
      ToSqlProxy::Borrowed(v) => Ok(v.try_into()?),
      ToSqlProxy::Owned(v) => Ok(v),
    }
  }
}

// Impl for rusqlite.
impl<'a> rusqlite::ToSql for ToSqlProxy<'a> {
  #[inline]
  fn to_sql(&self) -> Result<rusqlite::types::ToSqlOutput<'_>, rusqlite::Error> {
    Ok(match *self {
      ToSqlProxy::Borrowed(v) => rusqlite::types::ToSqlOutput::Borrowed(v.into()),
      ToSqlProxy::Owned(ref v) => rusqlite::types::ToSqlOutput::Borrowed(v.into()),
    })
  }
}
