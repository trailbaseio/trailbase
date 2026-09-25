use std::fmt::Debug;
use std::ops::Index;
use std::str::FromStr;

use crate::error::Error;
use crate::from_sql::{FromSql, FromSqlError};
use crate::value::Value;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ValueType {
  Undefined = 0,
  Integer = 1,
  Real,
  Text,
  Blob,
  Null,
}

pub(crate) type Rc<T> = triomphe::Arc<T>;
// pub(crate) type Rc<T> = std::sync::Arc<T>;

impl FromStr for ValueType {
  type Err = ();

  fn from_str(s: &str) -> std::result::Result<ValueType, Self::Err> {
    match s {
      "TEXT" => Ok(ValueType::Text),
      "INTEGER" => Ok(ValueType::Integer),
      "BLOB" => Ok(ValueType::Blob),
      "NULL" => Ok(ValueType::Null),
      "REAL" => Ok(ValueType::Real),
      _ => Err(()),
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
  pub(crate) name: compact_str::CompactString,
  pub(crate) decl_type: ValueType,
}

// TODO: Vec<Column> and Vec<Value> could be smallvecs. Vec<Row> probably not worth.
#[derive(Debug, Default)]
pub struct Rows(pub(crate) Vec<Row>, pub(crate) Rc<Vec<Column>>);

impl Rows {
  pub fn empty() -> Self {
    return Self(Vec::with_capacity(0), Rc::new(Vec::with_capacity(0)));
  }

  pub fn len(&self) -> usize {
    return self.0.len();
  }

  pub fn is_empty(&self) -> bool {
    return self.0.is_empty();
  }

  pub fn iter(&self) -> std::slice::Iter<'_, Row> {
    return self.0.iter();
  }

  pub fn get(&self, idx: usize) -> Option<&Row> {
    return self.0.get(idx);
  }

  pub fn last(&self) -> Option<&Row> {
    return self.0.last();
  }

  pub fn column_count(&self) -> usize {
    return self.1.len();
  }

  pub fn column_name(&self, idx: usize) -> Option<&str> {
    return self.1.get(idx).map(|c| c.name.as_str());
  }

  pub fn column_type(&self, idx: usize) -> Result<ValueType, Error> {
    return self
      .1
      .get(idx)
      .map(|c| c.decl_type)
      .ok_or_else(|| Error::InvalidColumnType {
        idx,
        name: self.column_name(idx).unwrap_or("?").to_string(),
        decl_type: None,
      });
  }
}

impl Index<usize> for Rows {
  type Output = Row;

  fn index(&self, idx: usize) -> &Self::Output {
    return &self.0[idx];
  }
}

impl IntoIterator for Rows {
  type Item = Row;
  type IntoIter = std::vec::IntoIter<Self::Item>;

  fn into_iter(self) -> Self::IntoIter {
    return self.0.into_iter();
  }
}

#[derive(Debug)]
pub struct Row(pub Vec<Value>, pub Rc<Vec<Column>>);

impl Row {
  pub fn split_off(&mut self, at: usize) -> Row {
    let split_values = self.0.split_off(at);
    let mut columns = (*self.1).clone();
    let split_columns = columns.split_off(at);
    self.1 = Rc::new(columns);
    return Row(split_values, Rc::new(split_columns));
  }

  pub fn len(&self) -> usize {
    return self.0.len();
  }

  pub fn is_empty(&self) -> bool {
    return self.0.is_empty();
  }

  pub fn column_count(&self) -> usize {
    return self.1.len();
  }

  pub fn column_name(&self, idx: usize) -> Option<&str> {
    return self.1.get(idx).map(|c| c.name.as_str());
  }

  pub fn last(&self) -> Option<&Value> {
    return self.0.last();
  }

  pub fn get<T>(&self, idx: usize) -> Result<T, FromSqlError>
  where
    T: FromSql,
  {
    let Some(v) = self.0.get(idx) else {
      return Err(FromSqlError::OutOfRange(idx as i64));
    };
    return T::column_result(v.into());
  }

  pub fn get_value(&self, idx: usize) -> Result<&Value, FromSqlError> {
    return self
      .0
      .get(idx)
      .ok_or_else(|| FromSqlError::OutOfRange(idx as i64));
  }

  // NOTE: This one currently doesn't make sense becaues FromSql forces a copy through ValueRef.
  // pub fn consume<T>(&mut self, idx: usize) -> Result<T, FromSqlError>
  // where
  //   T: FromSql,
  // {
  //   let Some(v) = self.0.get_mut(idx) else {
  //     return Err(FromSqlError::OutOfRange(idx as i64));
  //   };
  //   return T::column_result(v.into());
  // }

  pub fn consume_value(&mut self, idx: usize) -> Result<Value, FromSqlError> {
    let Some(v) = self.0.get_mut(idx) else {
      return Err(FromSqlError::OutOfRange(idx as i64));
    };
    return Ok(std::mem::take(v));
  }
}

impl Index<usize> for Row {
  type Output = Value;

  fn index(&self, idx: usize) -> &Self::Output {
    return &self.0[idx];
  }
}
