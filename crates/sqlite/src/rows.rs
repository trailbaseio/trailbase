use std::fmt::Debug;
use std::ops::Index;
use std::str::FromStr;

use crate::error::Error;
use crate::from_sql::{FromSql, FromSqlError};
use crate::value::Value;

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub enum ValueType {
  #[default]
  Undefined = 0,
  Integer = 1,
  Real,
  Text,
  Blob,
  Null,
}

pub(crate) type ColVec<T> = Vec<T>;
//pub(crate) type ColVec<T> = smallvec::SmallVec<[T; 16]>;
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
  pub name: compact_str::CompactString,
  pub decl_type: ValueType,
}

// TODO: Vec<Column> and Vec<Value> could be smallvecs. Vec<Row> probably not worth.
#[derive(Debug, Default)]
pub struct Rows {
  pub(crate) rows: Vec<Row>,
  pub(crate) columns: Rc<ColVec<Column>>,
}

impl Rows {
  pub fn empty() -> Self {
    return Self {
      rows: Vec::with_capacity(0),
      columns: Rc::new(ColVec::new()),
    };
  }

  pub fn len(&self) -> usize {
    return self.rows.len();
  }

  pub fn is_empty(&self) -> bool {
    return self.rows.is_empty();
  }

  #[inline]
  pub fn iter(&self) -> std::slice::Iter<'_, Row> {
    return self.rows.iter();
  }

  pub fn get(&self, idx: usize) -> Option<&Row> {
    return self.rows.get(idx);
  }

  pub fn last(&self) -> Option<&Row> {
    return self.rows.last();
  }

  pub fn column_count(&self) -> usize {
    return self.columns.len();
  }

  pub fn column(&self, idx: usize) -> Result<&Column, Error> {
    return self
      .columns
      .get(idx)
      .ok_or_else(|| Error::InvalidColumnType {
        idx,
        name: "?".to_string(),
        decl_type: None,
      });
  }
}

impl Index<usize> for Rows {
  type Output = Row;

  fn index(&self, idx: usize) -> &Self::Output {
    return &self.rows[idx];
  }
}

impl IntoIterator for Rows {
  type Item = Row;
  type IntoIter = std::vec::IntoIter<Self::Item>;

  #[inline]
  fn into_iter(self) -> Self::IntoIter {
    return self.rows.into_iter();
  }
}

#[derive(Debug)]
pub struct Row {
  pub(crate) values: ColVec<Value>,
  pub(crate) columns: Rc<ColVec<Column>>,
}

impl Row {
  pub fn split_off(&mut self, at: usize) -> Row {
    let split_values = self.values.split_off(at);
    //let split_values = split_off_smallvec(&mut self.values, at);

    let mut columns: ColVec<_> = (*self.columns).clone();
    // let split_columns = split_off_smallvec(&mut columns, at);
    let split_columns = columns.split_off(at);
    self.columns = Rc::new(columns);

    return Row {
      values: split_values,
      columns: Rc::new(split_columns),
    };
  }

  pub fn len(&self) -> usize {
    return self.values.len();
  }

  pub fn is_empty(&self) -> bool {
    return self.values.is_empty();
  }

  pub fn column_count(&self) -> usize {
    return self.columns.len();
  }

  #[inline]
  pub fn column_name(&self, idx: usize) -> Option<&str> {
    return self.columns.get(idx).map(|c| c.name.as_str());
  }

  #[inline]
  pub fn last(&self) -> Option<&Value> {
    return self.values.last();
  }

  #[inline]
  pub fn get<T>(&self, idx: usize) -> Result<T, FromSqlError>
  where
    T: FromSql,
  {
    let Some(v) = self.values.get(idx) else {
      return Err(FromSqlError::OutOfRange(idx as i64));
    };
    return T::column_result(v.into());
  }

  #[inline]
  pub fn get_value(&self, idx: usize) -> Result<&Value, FromSqlError> {
    return self
      .values
      .get(idx)
      .ok_or_else(|| FromSqlError::OutOfRange(idx as i64));
  }

  #[inline]
  pub fn get_value_mut(&mut self, idx: usize) -> Result<&Value, FromSqlError> {
    return self
      .values
      .get(idx)
      .ok_or_else(|| FromSqlError::OutOfRange(idx as i64));
  }

  // NOTE: This one currently doesn't make sense because FromSql forces a copy through ValueRef.
  // pub fn consume<T>(&mut self, idx: usize) -> Result<T, FromSqlError>
  // where
  //   T: FromSql,
  // {
  //   let Some(v) = self.0.get_mut(idx) else {
  //     return Err(FromSqlError::OutOfRange(idx as i64));
  //   };
  //   return T::column_result(v.into());
  // }

  #[inline]
  pub fn consume_value(&mut self, idx: usize) -> Result<Value, FromSqlError> {
    let Some(v) = self.values.get_mut(idx) else {
      return Err(FromSqlError::OutOfRange(idx as i64));
    };
    return Ok(std::mem::take(v));
  }

  pub fn into_values(self) -> ColVec<Value> {
    return self.values;
  }
}

impl Index<usize> for Row {
  type Output = Value;

  fn index(&self, idx: usize) -> &Self::Output {
    return &self.values[idx];
  }
}

#[inline]
#[allow(unused)]
fn split_off_smallvec<A>(vec: &mut smallvec::SmallVec<A>, at: usize) -> smallvec::SmallVec<A>
where
  A: smallvec::Array,
  A::Item: Clone,
{
  assert!(at <= vec.len(), "`at` out of bounds");
  let mut tail = smallvec::SmallVec::<A>::with_capacity(vec.len() - at);
  tail.extend(vec.drain(at..));
  return tail;
}
