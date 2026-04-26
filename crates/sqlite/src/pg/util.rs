use postgres::fallible_iterator::FallibleIterator;
use std::sync::Arc;

use crate::error::Error;
use crate::params::Params;
use crate::rows::{Column, Row, Rows};
use crate::statement::Statement;
use crate::to_sql::ToSqlProxy;
use crate::value::Value;

#[derive(Debug)]
pub struct PgStatement<'a> {
  #[allow(unused)]
  sql: &'a str,
  // TODO: Can we use ToSqlProxy here?
  params: &'a mut Vec<(usize, Value)>,
}

impl<'a> Statement for PgStatement<'a> {
  fn bind_parameter(&mut self, one_based_index: usize, param: ToSqlProxy<'_>) -> Result<(), Error> {
    self.params.push((one_based_index, param.try_into()?));
    return Ok(());
  }

  fn parameter_index(&self, _name: &str) -> Result<Option<usize>, Error> {
    return Err(Error::Other("not implemented: parse `self.sql`".into()));
  }
}

#[inline]
pub(crate) fn bind(sql: &str, params: impl Params) -> Result<Vec<Value>, Error> {
  let mut bound: Vec<(usize, Value)> = vec![];
  let mut stmt = PgStatement {
    sql: sql.as_ref(),
    params: &mut bound,
  };
  params.bind(&mut stmt)?;

  bound.sort_by(|a, b| {
    return a.0.cmp(&b.0);
  });

  // TODO: Do we need further validation, e.g. that indexes are consecutive?

  return Ok(bound.into_iter().map(|p| p.1).collect());
}

#[inline]
pub(crate) fn map_first<T>(
  mut rows: postgres::RowIter<'_>,
  f: impl (FnOnce(postgres::Row) -> Result<T, Error>) + Send + 'static,
) -> Result<Option<T>, Error>
where
  T: Send + 'static,
{
  if let Some(row) = rows.next()? {
    return Ok(Some(f(row)?));
  }
  return Ok(None);
}

pub fn from_rows(mut row_iter: postgres::RowIter) -> Result<Rows, Error> {
  let mut result = vec![];
  while let Some(row) = row_iter.next()? {
    // FIXME: Should be shared.
    let columns: Arc<Vec<Column>> = Arc::new(columns(&row));
    result.push(self::from_row(&row, columns.clone())?);
  }

  let columns = result
    .first()
    .map_or_else(|| Arc::new(Vec::new()), |row| row.1.clone());

  return Ok(Rows(result, columns));
}

pub(crate) fn from_row(row: &postgres::Row, cols: Arc<Vec<Column>>) -> Result<Row, Error> {
  #[cfg(debug_assertions)]
  if let Some(rc) = Some(columns(row))
    && rc.len() != cols.len()
  {
    // Apparently this can happen during schema manipulations, e.g. when deleting a column
    // :shrug:. We normalize everything to the same rows schema rather than dealing with
    // jagged tables.
    log::warn!("Rows/row column mismatch: {cols:?} vs {rc:?}");
  }

  // We have to access by index here, since names can be duplicate.
  let values = (0..cols.len())
    .map(|idx| row.try_get::<usize, Value>(idx).unwrap_or(Value::Null))
    .collect();

  return Ok(Row(values, cols));
}

#[inline]
pub(crate) fn columns(row: &postgres::Row) -> Vec<Column> {
  return row
    .columns()
    .into_iter()
    .map(|c| Column {
      name: c.name().to_string(),
      decl_type: match c.type_() {
        _ => None,
      },
    })
    .collect();
}
