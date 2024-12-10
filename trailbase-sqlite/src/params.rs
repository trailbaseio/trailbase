use rusqlite::types::ToSqlOutput;
use rusqlite::{types, Result, Statement};
use std::borrow::Cow;

pub type NamedParams = Vec<(Cow<'static, str>, types::Value)>;

// This strong typedef only exists to implement From<Option<T>>.
#[allow(missing_debug_implementations)]
pub enum ToSqlType {
  /// A borrowed SQLite-representable value.
  Borrowed(types::ValueRef<'static>),

  /// An owned SQLite-representable value.
  Owned(types::Value),
}

impl rusqlite::ToSql for ToSqlType {
  #[inline]
  fn to_sql(&self) -> Result<ToSqlOutput<'_>> {
    Ok(match *self {
      ToSqlType::Borrowed(v) => ToSqlOutput::Borrowed(v),
      ToSqlType::Owned(ref v) => ToSqlOutput::Borrowed(types::ValueRef::from(v)),
    })
  }
}

impl<T: ?Sized> From<&'static T> for ToSqlType
where
  &'static T: Into<types::ValueRef<'static>>,
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
                    None => ToSqlType::Owned(types::Value::Null),
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
from_value!(types::Value);

impl<const N: usize> From<[u8; N]> for ToSqlType {
  fn from(t: [u8; N]) -> Self {
    ToSqlType::Owned(types::Value::Blob(t.into()))
  }
}

pub trait Params {
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()>;
}

impl Params for () {
  fn bind(self, _stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    Ok(())
  }
}

impl Params for Vec<(String, types::Value)> {
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    for (name, v) in self {
      if let Some(idx) = stmt.parameter_index(&name)? {
        stmt.raw_bind_parameter(idx, v)?;
      };
    }
    return Ok(());
  }
}

impl Params for NamedParams {
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(&name)? else {
        continue;
      };
      stmt.raw_bind_parameter(idx, v)?;
    }
    return Ok(());
  }
}

impl Params for Vec<(&str, types::Value)> {
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.raw_bind_parameter(idx, v)?;
    }
    return Ok(());
  }
}

impl Params for &[(&str, types::Value)] {
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.raw_bind_parameter(idx, v)?;
    }
    return Ok(());
  }
}

impl<const N: usize> Params for [(&str, types::Value); N] {
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.raw_bind_parameter(idx, v)?;
    }
    return Ok(());
  }
}

impl<const N: usize> Params for [(&str, ToSqlType); N] {
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.raw_bind_parameter(idx, v)?;
    }
    return Ok(());
  }
}

impl Params for Vec<types::Value> {
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    for (idx, p) in self.into_iter().enumerate() {
      stmt.raw_bind_parameter(idx + 1, p)?;
    }
    return Ok(());
  }
}

impl Params for &[types::Value] {
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    for (idx, p) in self.iter().enumerate() {
      stmt.raw_bind_parameter(idx + 1, p)?;
    }
    return Ok(());
  }
}

impl<const N: usize> Params for [ToSqlType; N] {
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    for (idx, p) in self.into_iter().enumerate() {
      stmt.raw_bind_parameter(idx + 1, p)?;
    }
    return Ok(());
  }
}

impl<T, const N: usize> Params for &[T; N]
where
  T: rusqlite::ToSql + Send + Sync,
{
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    for (idx, p) in self.iter().enumerate() {
      stmt.raw_bind_parameter(idx + 1, p)?;
    }
    return Ok(());
  }
}

impl<T> Params for (T,)
where
  T: rusqlite::ToSql + Send + Sync,
{
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    return stmt.raw_bind_parameter(1, self.0);
  }
}

impl<const N: usize> Params for [types::Value; N] {
  fn bind(self, stmt: &mut Statement<'_>) -> rusqlite::Result<()> {
    for (idx, p) in self.into_iter().enumerate() {
      stmt.raw_bind_parameter(idx + 1, p)?;
    }
    return Ok(());
  }
}
