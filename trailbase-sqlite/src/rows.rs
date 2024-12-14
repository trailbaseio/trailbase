use rusqlite::{types, Statement};
use std::{fmt::Debug, str::FromStr, sync::Arc};

use crate::error::Error;

#[derive(Debug, Copy, Clone)]
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

#[allow(unused)]
#[derive(Debug)]
pub struct Column {
  name: String,
  decl_type: Option<ValueType>,
}

#[derive(Debug)]
pub struct Rows(pub(crate) Vec<Row>, pub(crate) Arc<Vec<Column>>);

pub(crate) fn columns(stmt: &Statement<'_>) -> Vec<Column> {
  return stmt
    .columns()
    .into_iter()
    .map(|c| Column {
      name: c.name().to_string(),
      decl_type: c.decl_type().and_then(|s| ValueType::from_str(s).ok()),
    })
    .collect();
}

impl Rows {
  pub fn from_rows(mut rows: rusqlite::Rows) -> rusqlite::Result<Self> {
    let columns: Arc<Vec<Column>> = Arc::new(rows.as_ref().map_or(vec![], columns));

    let mut result = vec![];
    while let Some(row) = rows.next()? {
      result.push(Row::from_row(row, Some(columns.clone()))?);
    }

    Ok(Self(result, columns))
  }

  #[cfg(test)]
  pub fn len(&self) -> usize {
    self.0.len()
  }

  pub fn iter(&self) -> std::slice::Iter<'_, Row> {
    self.0.iter()
  }

  pub fn column_count(&self) -> usize {
    self.1.len()
  }

  pub fn column_names(&self) -> Vec<&str> {
    self.1.iter().map(|s| s.name.as_str()).collect()
  }

  pub fn column_name(&self, idx: usize) -> Option<&str> {
    self.1.get(idx).map(|c| c.name.as_str())
  }

  pub fn column_type(&self, idx: usize) -> std::result::Result<ValueType, rusqlite::Error> {
    if let Some(c) = self.1.get(idx) {
      return c.decl_type.ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(
          idx,
          self.column_name(idx).unwrap_or("?").to_string(),
          types::Type::Null,
        )
      });
    }
    return Err(rusqlite::Error::InvalidColumnType(
      idx,
      self.column_name(idx).unwrap_or("?").to_string(),
      types::Type::Null,
    ));
  }
}

#[derive(Debug)]
pub struct Row(Vec<types::Value>, Arc<Vec<Column>>);

impl Row {
  pub fn from_row(row: &rusqlite::Row, cols: Option<Arc<Vec<Column>>>) -> rusqlite::Result<Self> {
    let columns = cols.unwrap_or_else(|| Arc::new(columns(row.as_ref())));

    let count = columns.len();
    let mut values = Vec::<types::Value>::with_capacity(count);
    for idx in 0..count {
      values.push(row.get_ref(idx)?.into());
    }

    Ok(Self(values, columns))
  }

  pub fn get<T>(&self, idx: usize) -> types::FromSqlResult<T>
  where
    T: types::FromSql,
  {
    let val = self
      .0
      .get(idx)
      .ok_or_else(|| types::FromSqlError::Other("Index out of bounds".into()))?;
    T::column_result(val.into())
  }

  pub fn get_value(&self, idx: usize) -> Result<types::Value, Error> {
    self
      .0
      .get(idx)
      .ok_or_else(|| Error::Other("Index out of bounds".into()))
      .cloned()
  }

  pub fn column_count(&self) -> usize {
    self.0.len()
  }

  pub fn column_names(&self) -> Vec<&str> {
    self.1.iter().map(|s| s.name.as_str()).collect()
  }

  pub fn column_name(&self, idx: usize) -> Option<&str> {
    self.1.get(idx).map(|c| c.name.as_str())
  }
}
