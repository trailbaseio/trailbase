use crate::Error;
use crate::value::Value;
use stoolap::ToParam;

impl stoolap::ToParam for Value {
  fn to_param(&self) -> stoolap::Value {
    return match self {
      Value::Null => stoolap::Value::Null(Default::default()),
      Value::Integer(i) => stoolap::Value::Integer(*i),
      Value::Real(f) => stoolap::Value::Float(*f),
      Value::Text(s) => stoolap::Value::Text(s.into()),
      // Stoolap doesn't have built-in blob support.
      Value::Blob(b) => stoolap::Value::Extension(stoolap::common::CompactArc::from_slice(&b)),
    };
  }
}

pub struct Params {
  params: Vec<(usize, Value)>,
}

impl Params {
  pub fn new() -> Self {
    return Self { params: vec![] };
  }
}

impl crate::statement::Statement for Params {
  fn bind_parameter(
    &mut self,
    one_based_index: usize,
    param: crate::to_sql::ToSqlProxy,
  ) -> Result<(), Error> {
    self.params.push((one_based_index, param.try_into()?));
    return Ok(());
  }

  /// Will return Err if `name` is invalid. Will return Ok(None) if the name
  /// is valid but not a bound parameter of this statement.
  fn parameter_index(&self, _name: &str) -> Result<Option<usize>, Error> {
    return Err(Error::NotImplemented);
  }
}

impl stoolap::Params for Params {
  fn into_params(self) -> stoolap::ParamVec {
    return self.params.into_iter().map(|p| p.1.to_param()).collect();
  }
}

pub fn map_params(params: impl crate::params::Params) -> Result<Params, Error> {
  let mut p = Params::new();
  params.bind(&mut p)?;
  return Ok(p);
}
