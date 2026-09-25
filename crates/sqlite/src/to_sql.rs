use crate::value::{Value, ValueRef};

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
        impl From<Option<$t>> for ToSqlProxy<'_> {
            #[inline]
            fn from(t: Option<$t>) -> Self {
                match t {
                    Some(t) => ToSqlProxy::Owned(t.into()),
                    None => ToSqlProxy::Owned(Value::Null),
                }
            }
        }
    )
);

from_value!(String);
from_value!(bool);
from_value!(i64);
from_value!(f64);
from_value!(Vec<u8>);
from_value!(Value);

// impl<'a> From<Blob> for ToSqlProxy<'a> {
//   fn from(t: Blob) -> Self {
//     return ToSqlProxy::Owned(t.into());
//   }
// }

impl<'a> From<[u8; 16]> for ToSqlProxy<'a> {
  fn from(t: [u8; 16]) -> Self {
    return ToSqlProxy::Owned(t.into());
  }
}

impl<'a, const N: usize> From<&'a [u8; N]> for ToSqlProxy<'a> {
  fn from(t: &'a [u8; N]) -> Self {
    ToSqlProxy::Borrowed(ValueRef::Blob(t))
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
