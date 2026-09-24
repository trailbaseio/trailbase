use std::iter::zip;
use std::sync::Arc;
use stoolap::ToParam;

use crate::Error;
use crate::rows::{Column, Row, Rows, ValueType};
use crate::value::Value;

// impl<T: stoolap::FromValue> crate::from_sql::FromSql for T {
//   fn column_result(value: crate::ValueRef<'_>) -> crate::from_sql::FromSqlResult<Self> {}
// }

// impl<T: crate::from_sql::FromSql> stoolap::FromRow for T {
//   fn from_row(row: &stoolap::ResultRow) -> stoolap::Result<T> {
//       T.
//       row
//   }
// }

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

pub fn from_rows(mut rows: stoolap::Rows) -> Result<Rows, Error> {
  let Some(Ok(first_row)) = rows.next() else {
    return Ok(Rows::default());
  };

  let columns: Arc<Vec<Column>> = Arc::new(columns(&first_row));
  let mut result = vec![self::from_row(first_row, columns.clone())?];
  while let Some(row) = rows.next() {
    result.push(self::from_row(row?, columns.clone())?);
  }

  return Ok(Rows(result, columns));
}

pub fn from_row(row: stoolap::ResultRow, cols: Arc<Vec<Column>>) -> Result<Row, Error> {
  use stoolap::Value as SValue;
  let values = row
    .into_inner()
    .into_iter()
    .map(|v| {
      return match v {
        SValue::Null(_) => Value::Null,
        SValue::Boolean(b) => Value::Integer(if b { 1 } else { 0 }),
        SValue::Integer(i) => Value::Integer(i),
        SValue::Float(f) => Value::Real(f),
        SValue::Text(s) => Value::Text(s.to_string()),
        SValue::Timestamp(ts) => Value::Integer(ts.timestamp_millis()),
        SValue::Extension(b) => Value::Blob(b.to_vec()),
      };
    })
    .collect();

  return Ok(Row(values, cols));
}

#[inline]
pub(crate) fn columns(row: &stoolap::ResultRow) -> Vec<Column> {
  use stoolap::Value;

  return row
    .columns()
    .iter()
    .enumerate()
    .map(|(idx, name)| {
      return match row.get_value(idx) {
        None | Some(Value::Null(_)) => Column {
          name: name.clone(),
          decl_type: Some(ValueType::Null),
        },
        Some(Value::Integer(_)) => Column {
          name: name.clone(),
          decl_type: Some(ValueType::Integer),
        },
        Some(Value::Boolean(_)) => Column {
          name: name.clone(),
          decl_type: Some(ValueType::Integer),
        },
        Some(Value::Timestamp(_)) => Column {
          name: name.clone(),
          decl_type: Some(ValueType::Integer),
        },
        Some(Value::Float(_)) => Column {
          name: name.clone(),
          decl_type: Some(ValueType::Real),
        },
        Some(Value::Text(_)) => Column {
          name: name.clone(),
          decl_type: Some(ValueType::Text),
        },
        Some(Value::Extension(_)) => Column {
          name: name.clone(),
          decl_type: Some(ValueType::Blob),
        },
      };
    })
    .collect();
}
