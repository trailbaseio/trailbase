use rusqlite::types;
use std::fmt::Debug;
use std::ops::Index;
use std::str::FromStr;
use std::sync::Arc;

use crate::error::Error;
use crate::value::Value;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ValueType {
  Integer = 1,
  Real,
  Text,
  Blob,
  Null,
}

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
  pub(crate) name: String,
  pub(crate) decl_type: Option<ValueType>,
}

#[derive(Debug)]
pub struct Rows(pub(crate) Vec<Row>, pub(crate) Arc<Vec<Column>>);

impl Rows {
  pub fn empty() -> Self {
    return Self(vec![], Arc::new(vec![]));
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
      .and_then(|c| c.decl_type)
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
pub struct Row(pub Vec<Value>, pub Arc<Vec<Column>>);

impl Row {
  pub fn split_off(&mut self, at: usize) -> Row {
    let split_values = self.0.split_off(at);
    let mut columns = (*self.1).clone();
    let split_columns = columns.split_off(at);
    self.1 = Arc::new(columns);
    return Row(split_values, Arc::new(split_columns));
  }

  pub fn get<T>(&self, idx: usize) -> types::FromSqlResult<T>
  where
    T: types::FromSql,
  {
    let Some(value) = self.0.get(idx) else {
      return Err(types::FromSqlError::OutOfRange(idx as i64));
    };
    return T::column_result(value.into());
  }

  pub fn get_value(&self, idx: usize) -> Option<&Value> {
    return self.0.get(idx);
  }

  pub fn len(&self) -> usize {
    return self.0.len();
  }

  pub fn is_empty(&self) -> bool {
    return self.0.is_empty();
  }

  pub fn last(&self) -> Option<&Value> {
    return self.0.last();
  }

  pub fn column_count(&self) -> usize {
    return self.1.len();
  }

  pub fn column_name(&self, idx: usize) -> Option<&str> {
    return self.1.get(idx).map(|c| c.name.as_str());
  }
}

impl Index<usize> for Row {
  type Output = Value;

  fn index(&self, idx: usize) -> &Self::Output {
    return &self.0[idx];
  }
}
