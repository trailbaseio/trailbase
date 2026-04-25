use postgres::fallible_iterator::FallibleIterator;
use std::sync::Arc;

use crate::error::Error;
use crate::rows::{Column, Row, Rows};
use crate::value::Value;

// #[inline]
// pub fn get_value<T: FromSql>(row: &postgres::Row, idx: usize) -> Result<T, Error> {
//     postgres::types::Json
//   let value = row.get(idx)?;
//
//   return FromSql::column_result(value.into()).map_err(|err| {
//     use rusqlite::Error as RError;
//
//     return Error::Rusqlite(match err {
//       FromSqlError::InvalidType => {
//         RError::InvalidColumnType(idx, "<unknown>".into(), value.data_type())
//       }
//       FromSqlError::OutOfRange(i) => RError::IntegralValueOutOfRange(idx, i),
//       FromSqlError::Utf8Error(err) => RError::Utf8Error(idx, err),
//       FromSqlError::Other(err) => RError::FromSqlConversionFailure(idx, value.data_type(), err),
//       FromSqlError::InvalidBlobSize { .. } => {
//         RError::FromSqlConversionFailure(idx, value.data_type(), Box::new(err))
//       }
//     });
//   });
// }

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
