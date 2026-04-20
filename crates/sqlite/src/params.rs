use std::borrow::Cow;

use crate::error::Error;
use crate::statement::Statement;
use crate::to_sql::ToSqlProxy;
use crate::value::Value;

pub trait Params {
  fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error>;
}

pub type NamedParams = Vec<(Cow<'static, str>, Value)>;
pub type NamedParamRef<'a> = (Cow<'static, str>, ToSqlProxy<'a>);
pub type NamedParamsRef<'a> = &'a [NamedParamRef<'a>];

impl Params for () {
  fn bind<S: Statement>(self, _stmt: &mut S) -> Result<(), Error> {
    Ok(())
  }
}

impl Params for Vec<(String, Value)> {
  fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
    for (name, v) in self {
      if let Some(idx) = stmt.parameter_index(&name)? {
        stmt.bind_parameter(idx, v.into())?;
      };
    }
    return Ok(());
  }
}

impl Params for NamedParams {
  fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(&name)? else {
        continue;
      };
      stmt.bind_parameter(idx, v.into())?;
    }
    return Ok(());
  }
}

impl Params for Vec<(&str, Value)> {
  fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.bind_parameter(idx, v.into())?;
    }
    return Ok(());
  }
}

impl Params for &[(&str, Value)] {
  fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.bind_parameter(idx, v.into())?;
    }
    return Ok(());
  }
}

// impl Params for NamedParamsRef<'_> {
//   fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
//     for (name, v) in self {
//       let Some(idx) = stmt.parameter_index(name)? else {
//         continue;
//       };
//       stmt.bind_parameter(idx, v)?;
//     }
//     return Ok(());
//   }
// }

impl<const N: usize> Params for [(&str, Value); N] {
  fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.bind_parameter(idx, v.into())?;
    }
    return Ok(());
  }
}

impl<'a, const N: usize> Params for [(&str, ToSqlProxy<'a>); N] {
  fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
    for (name, v) in self {
      let Some(idx) = stmt.parameter_index(name)? else {
        continue;
      };
      stmt.bind_parameter(idx, v)?;
    }
    return Ok(());
  }
}

impl Params for Vec<Value> {
  fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
    for (idx, v) in self.into_iter().enumerate() {
      stmt.bind_parameter(idx + 1, v.into())?;
    }
    return Ok(());
  }
}

// impl Params for &[Value] {
//   fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
//     for (idx, v) in self.iter().enumerate() {
//       stmt.bind_parameter(idx + 1, v)?;
//     }
//     return Ok(());
//   }
// }

impl<'a, const N: usize> Params for [ToSqlProxy<'a>; N] {
  fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
    for (idx, v) in self.into_iter().enumerate() {
      stmt.bind_parameter(idx + 1, v)?;
    }
    return Ok(());
  }
}

// impl<T, const N: usize> Params for &[T; N]
// where
//   T: rusqlite::ToSql + Send + Sync,
// {
//   fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
//     for (idx, v) in self.iter().enumerate() {
//       stmt.bind_parameter(idx + 1, v)?;
//     }
//     return Ok(());
//   }
// }

impl<'a, T> Params for (T,)
where
  T: Into<ToSqlProxy<'a>> + Send,
{
  fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
    return Ok(stmt.bind_parameter(1, self.0.into())?);
  }
}

// impl<T: Params + Clone> Params for &T {
//   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), rusqlite::Error> {
//     return self.clone().bind(stmt);
//   }
// }

impl<const N: usize> Params for [Value; N] {
  fn bind<S: Statement>(self, stmt: &mut S) -> Result<(), Error> {
    for (idx, v) in self.into_iter().enumerate() {
      stmt.bind_parameter(idx + 1, v.into())?;
    }
    return Ok(());
  }
}

// OLD

// pub trait Params {
//   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error>;
// }
//
// impl Params for () {
//   fn bind(self, _stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
//     Ok(())
//   }
// }
//
// impl Params for Vec<(String, Value)> {
//   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
//     for (name, v) in self {
//       if let Some(idx) = stmt.parameter_index(&name)? {
//         stmt.raw_bind_parameter(idx, rusqlite::types::Value::from(v))?;
//       };
//     }
//     return Ok(());
//   }
// }
//
// impl Params for NamedParams {
//   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
//     for (name, v) in self {
//       let Some(idx) = stmt.parameter_index(&name)? else {
//         continue;
//       };
//       stmt.raw_bind_parameter(idx, rusqlite::types::Value::from(v))?;
//     }
//     return Ok(());
//   }
// }
//
// impl Params for Vec<(&str, Value)> {
//   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
//     for (name, v) in self {
//       let Some(idx) = stmt.parameter_index(name)? else {
//         continue;
//       };
//       stmt.raw_bind_parameter(idx, rusqlite::types::Value::from(v))?;
//     }
//     return Ok(());
//   }
// }
//
// // impl Params for &[(&str, Value)] {
// //   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
// //     for (name, v) in self {
// //       let Some(idx) = stmt.parameter_index(name)? else {
// //         continue;
// //       };
// //       stmt.raw_bind_parameter(idx, v)?;
// //     }
// //     return Ok(());
// //   }
// // }
//
// // impl Params for NamedParamsRef<'_> {
// //   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
// //     for (name, v) in self {
// //       let Some(idx) = stmt.parameter_index(name)? else {
// //         continue;
// //       };
// //       stmt.raw_bind_parameter(idx, v)?;
// //     }
// //     return Ok(());
// //   }
// // }
//
// impl<const N: usize> Params for [(&str, Value); N] {
//   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
//     for (name, v) in self {
//       let Some(idx) = stmt.parameter_index(name)? else {
//         continue;
//       };
//       stmt.raw_bind_parameter(idx, rusqlite::types::Value::from(v))?;
//     }
//     return Ok(());
//   }
// }
//
// impl<'a, const N: usize> Params for [(&str, ToSqlProxy<'a>); N] {
//   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
//     for (name, v) in self {
//       let Some(idx) = stmt.parameter_index(name)? else {
//         continue;
//       };
//       stmt.raw_bind_parameter(idx, v)?;
//     }
//     return Ok(());
//   }
// }
//
// impl Params for Vec<Value> {
//   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
//     for (idx, v) in self.into_iter().enumerate() {
//       stmt.raw_bind_parameter(idx + 1, rusqlite::types::Value::from(v))?;
//     }
//     return Ok(());
//   }
// }
//
// // impl Params for &[Value] {
// //   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
// //     for (idx, v) in self.iter().enumerate() {
// //       stmt.raw_bind_parameter(idx + 1, v)?;
// //     }
// //     return Ok(());
// //   }
// // }
//
// impl<'a, const N: usize> Params for [ToSqlProxy<'a>; N] {
//   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
//     for (idx, p) in self.into_iter().enumerate() {
//       stmt.raw_bind_parameter(idx + 1, p)?;
//     }
//     return Ok(());
//   }
// }
//
// // impl<T, const N: usize> Params for &[T; N]
// // where
// //   T: rusqlite::ToSql + Send + Sync,
// // {
// //   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
// //     for (idx, p) in self.iter().enumerate() {
// //       stmt.raw_bind_parameter(idx + 1, p)?;
// //     }
// //     return Ok(());
// //   }
// // }
//
// impl<T> Params for (T,)
// where
//   T: rusqlite::ToSql + Send + Sync,
// {
//   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
//     return Ok(stmt.raw_bind_parameter(1, self.0)?);
//   }
// }
//
// // impl<T: Params + Clone> Params for &T {
// //   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), rusqlite::Error> {
// //     return self.clone().bind(stmt);
// //   }
// // }
//
// impl<const N: usize> Params for [Value; N] {
//   fn bind(self, stmt: &mut rusqlite::Statement<'_>) -> Result<(), Error> {
//     for (idx, v) in self.into_iter().enumerate() {
//       stmt.raw_bind_parameter(idx + 1, rusqlite::types::Value::from(v))?;
//     }
//     return Ok(());
//   }
// }
