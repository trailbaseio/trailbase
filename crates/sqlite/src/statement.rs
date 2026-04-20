use crate::error::Error;
use crate::to_sql::ToSqlProxy;

pub trait Statement {
  fn bind_parameter(&mut self, one_based_index: usize, param: ToSqlProxy) -> Result<(), Error>;
  fn parameter_index(&self, name: &str) -> Result<Option<usize>, Error>;
}

impl<'a> Statement for rusqlite::Statement<'a> {
  #[inline]
  fn bind_parameter(&mut self, one_based_index: usize, param: ToSqlProxy) -> Result<(), Error> {
    return Ok(self.raw_bind_parameter(one_based_index, param)?);
  }

  #[inline]
  fn parameter_index(&self, name: &str) -> Result<Option<usize>, Error> {
    return Ok(self.parameter_index(name)?);
  }
}
