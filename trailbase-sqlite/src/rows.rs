use rusqlite::{types, Statement};
use std::fmt::Debug;
use std::ops::Index;
use std::str::FromStr;
use std::sync::Arc;

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

#[derive(Debug, Clone)]
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

    return Ok(Self(result, columns));
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

  pub fn column_names(&self) -> Vec<&str> {
    return self.1.iter().map(|s| s.name.as_str()).collect();
  }

  pub fn column_name(&self, idx: usize) -> Option<&str> {
    return self.1.get(idx).map(|c| c.name.as_str());
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

impl Index<usize> for Rows {
  type Output = Row;

  fn index(&self, idx: usize) -> &Self::Output {
    return &self.0[idx];
  }
}

#[derive(Debug)]
pub struct Row(Vec<types::Value>, Arc<Vec<Column>>);

impl Row {
  pub fn from_row(row: &rusqlite::Row, cols: Option<Arc<Vec<Column>>>) -> rusqlite::Result<Self> {
    let columns = cols.unwrap_or_else(|| Arc::new(columns(row.as_ref())));

    let values = (0..columns.len())
      .map(|idx| Ok(row.get_ref(idx)?.into()))
      .collect::<Result<Vec<types::Value>, rusqlite::Error>>()?;

    return Ok(Self(values, columns));
  }

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

  pub fn get_value(&self, idx: usize) -> Option<&types::Value> {
    return self.0.get(idx);
  }

  pub fn len(&self) -> usize {
    assert_eq!(self.1.len(), self.0.len());
    return self.0.len();
  }

  pub fn is_empty(&self) -> bool {
    return self.0.is_empty();
  }

  pub fn column_count(&self) -> usize {
    assert_eq!(self.1.len(), self.0.len());
    return self.1.len();
  }

  pub fn column_names(&self) -> Vec<&str> {
    return self.1.iter().map(|s| s.name.as_str()).collect();
  }

  pub fn column_name(&self, idx: usize) -> Option<&str> {
    return self.1.get(idx).map(|c| c.name.as_str());
  }
}

impl Index<usize> for Row {
  type Output = types::Value;

  fn index(&self, idx: usize) -> &Self::Output {
    return &self.0[idx];
  }
}
