use crate::value::{Value, ValueRef};

// Proxy/strong-typedef that only exists to implement `params!`/`named_params!`.
#[allow(missing_debug_implementations)]
pub enum ToSqlProxy {
  /// A borrowed SQLite-representable value.
  Borrowed(ValueRef<'static>),

  /// An owned SQLite-representable value.
  Owned(Value),
}

impl<T: ?Sized> From<&'static T> for ToSqlProxy
where
  &'static T: Into<ValueRef<'static>>,
{
  #[inline]
  fn from(t: &'static T) -> Self {
    ToSqlProxy::Borrowed(t.into())
  }
}

macro_rules! from_value(
    ($t:ty) => (
        impl From<$t> for ToSqlProxy {
            #[inline]
            fn from(t: $t) -> Self { ToSqlProxy::Owned(t.into())}
        }
        impl From<Option<$t>> for ToSqlProxy {
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

impl<const N: usize> From<[u8; N]> for ToSqlProxy {
  fn from(t: [u8; N]) -> Self {
    ToSqlProxy::Owned(Value::Blob(t.into()))
  }
}

// Impl for rusqlite.
impl rusqlite::ToSql for ToSqlProxy {
  #[inline]
  fn to_sql(&self) -> Result<rusqlite::types::ToSqlOutput<'_>, rusqlite::Error> {
    Ok(match *self {
      ToSqlProxy::Borrowed(v) => rusqlite::types::ToSqlOutput::Borrowed(v.into()),
      ToSqlProxy::Owned(ref v) => rusqlite::types::ToSqlOutput::Borrowed(v.into()),
    })
  }
}
