use rusqlite::{Statement, types};
use std::borrow::Cow;

use crate::to_sql::ToSqlProxy;
use crate::value::Value;
// use crate::statement::Statement;

pub trait Params {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error>;
}

pub type NamedParams = Vec<(Cow<'static, str>, Value)>;
pub type NamedParamRef<'a> = (Cow<'static, str>, types::ToSqlOutput<'a>);
pub type NamedParamsRef<'a> = &'a [NamedParamRef<'a>];

impl Params for () {
  fn bind(self, _stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    Ok(())
  }
}

impl Params for Vec<(String, Value)> {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    for (name, v) in self {
      if let Some(idx) = stmt.parameter_index(&name)? {
        stmt.raw_bind_parameter(idx, rusqlite::types::Value::from(v))?;
      };
    }
    return Ok(());
  }
}

impl Params for NamedParams {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(&name)? else {
        continue;
      };
      stmt.raw_bind_parameter(idx, rusqlite::types::Value::from(v))?;
    }
    return Ok(());
  }
}

impl Params for Vec<(&str, Value)> {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.raw_bind_parameter(idx, rusqlite::types::Value::from(v))?;
    }
    return Ok(());
  }
}

impl Params for &[(&str, Value)] {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.raw_bind_parameter(idx, v)?;
    }
    return Ok(());
  }
}

impl Params for NamedParamsRef<'_> {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.raw_bind_parameter(idx, v)?;
    }
    return Ok(());
  }
}

impl<const N: usize> Params for [(&str, Value); N] {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.raw_bind_parameter(idx, rusqlite::types::Value::from(v))?;
    }
    return Ok(());
  }
}

impl<'a, const N: usize> Params for [(&str, ToSqlProxy<'a>); N] {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.raw_bind_parameter(idx, v)?;
    }
    return Ok(());
  }
}

impl Params for Vec<Value> {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    for (idx, v) in self.into_iter().enumerate() {
      stmt.raw_bind_parameter(idx + 1, rusqlite::types::Value::from(v))?;
    }
    return Ok(());
  }
}

impl Params for &[Value] {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    for (idx, v) in self.iter().enumerate() {
      stmt.raw_bind_parameter(idx + 1, v)?;
    }
    return Ok(());
  }
}

impl<'a, const N: usize> Params for [ToSqlProxy<'a>; N] {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
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
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
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
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    return stmt.raw_bind_parameter(1, self.0);
  }
}

// impl<T: Params + Clone> Params for &T {
//   fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
//     return self.clone().bind(stmt);
//   }
// }

impl<const N: usize> Params for [Value; N] {
  fn bind(self, stmt: &mut Statement<'_>) -> Result<(), rusqlite::Error> {
    for (idx, v) in self.into_iter().enumerate() {
      stmt.raw_bind_parameter(idx + 1, rusqlite::types::Value::from(v))?;
    }
    return Ok(());
  }
}
