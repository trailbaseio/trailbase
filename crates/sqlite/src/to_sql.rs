use crate::value::{Value, ValueRef};

// This strong typedef only exists to implement From<Option<T>>.
#[allow(missing_debug_implementations)]
pub enum ToSqlType {
  /// A borrowed SQLite-representable value.
  Borrowed(ValueRef<'static>),

  /// An owned SQLite-representable value.
  Owned(Value),
}

impl<T: ?Sized> From<&'static T> for ToSqlType
where
  &'static T: Into<ValueRef<'static>>,
{
  #[inline]
  fn from(t: &'static T) -> Self {
    ToSqlType::Borrowed(t.into())
  }
}

macro_rules! from_value(
    ($t:ty) => (
        impl From<$t> for ToSqlType {
            #[inline]
            fn from(t: $t) -> Self { ToSqlType::Owned(t.into())}
        }
        impl From<Option<$t>> for ToSqlType {
            #[inline]
            fn from(t: Option<$t>) -> Self {
                match t {
                    Some(t) => ToSqlType::Owned(t.into()),
                    None => ToSqlType::Owned(Value::Null),
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

impl<const N: usize> From<[u8; N]> for ToSqlType {
  fn from(t: [u8; N]) -> Self {
    ToSqlType::Owned(Value::Blob(t.into()))
  }
}

// Impl for rusqlite.
impl rusqlite::ToSql for ToSqlType {
  #[inline]
  fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
    use rusqlite::types::ToSqlOutput;

    Ok(match *self {
      ToSqlType::Borrowed(v) => ToSqlOutput::Borrowed(v.into()),
      ToSqlType::Owned(ref v) => ToSqlOutput::Borrowed(v.into()),
    })
  }
}
