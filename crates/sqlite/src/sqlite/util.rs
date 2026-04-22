use rusqlite::hooks::PreUpdateCase;
use std::str::FromStr;
use std::sync::Arc;

use crate::SyncConnectionTrait;
use crate::database::Database;
use crate::error::Error;
use crate::from_sql::{FromSql, FromSqlError};
use crate::rows::{Column, Row, Rows, ValueType};
use crate::value::Value;

#[inline]
pub(crate) fn map_first<T>(
  mut rows: rusqlite::Rows<'_>,
  f: impl (FnOnce(&rusqlite::Row<'_>) -> Result<T, Error>) + Send + 'static,
) -> Result<Option<T>, Error>
where
  T: Send + 'static,
{
  if let Some(row) = rows.next()? {
    return Ok(Some(f(row)?));
  }
  return Ok(None);
}

#[inline]
pub fn get_value<T: FromSql>(row: &rusqlite::Row<'_>, idx: usize) -> Result<T, Error> {
  let value = row.get_ref(idx)?;

  return FromSql::column_result(value.into()).map_err(|err| {
    use rusqlite::Error as RError;

    return Error::Rusqlite(match err {
      FromSqlError::InvalidType => {
        RError::InvalidColumnType(idx, "<unknown>".into(), value.data_type())
      }
      FromSqlError::OutOfRange(i) => RError::IntegralValueOutOfRange(idx, i),
      FromSqlError::Utf8Error(err) => RError::Utf8Error(idx, err),
      FromSqlError::Other(err) => RError::FromSqlConversionFailure(idx, value.data_type(), err),
      FromSqlError::InvalidBlobSize { .. } => {
        RError::FromSqlConversionFailure(idx, value.data_type(), Box::new(err))
      }
    });
  });
}

pub fn from_rows(mut rows: rusqlite::Rows) -> Result<Rows, Error> {
  let columns: Arc<Vec<Column>> = Arc::new(rows.as_ref().map_or_else(Vec::new, columns));

  let mut result = vec![];
  while let Some(row) = rows.next()? {
    result.push(self::from_row(row, columns.clone())?);
  }

  return Ok(Rows(result, columns));
}

pub(super) fn from_row(row: &rusqlite::Row, cols: Arc<Vec<Column>>) -> Result<Row, Error> {
  #[cfg(debug_assertions)]
  if let Some(rc) = Some(columns(row.as_ref()))
    && rc.len() != cols.len()
  {
    // Apparently this can happen during schema manipulations, e.g. when deleting a column
    // :shrug:. We normalize everything to the same rows schema rather than dealing with
    // jagged tables.
    log::warn!("Rows/row column mismatch: {cols:?} vs {rc:?}");
  }

  // We have to access by index here, since names can be duplicate.
  let values = (0..cols.len())
    .map(|idx| row.get(idx).unwrap_or(Value::Null))
    .collect();

  return Ok(Row(values, cols));
}

#[inline]
pub(super) fn columns(stmt: &rusqlite::Statement<'_>) -> Vec<Column> {
  return stmt
    .columns()
    .into_iter()
    .map(|c| Column {
      name: c.name().to_string(),
      decl_type: c.decl_type().and_then(|s| ValueType::from_str(s).ok()),
    })
    .collect();
}

#[inline]
pub fn extract_row_id(case: &PreUpdateCase) -> Option<i64> {
  return match case {
    PreUpdateCase::Insert(accessor) => Some(accessor.get_new_row_id()),
    PreUpdateCase::Delete(accessor) => Some(accessor.get_old_row_id()),
    PreUpdateCase::Update {
      new_value_accessor: accessor,
      ..
    } => Some(accessor.get_new_row_id()),
    PreUpdateCase::Unknown => None,
  };
}

#[inline]
pub fn extract_record_values(case: &PreUpdateCase) -> Option<Vec<Value>> {
  return Some(match case {
    PreUpdateCase::Insert(accessor) => (0..accessor.get_column_count())
      .map(|idx| -> Value {
        accessor
          .get_new_column_value(idx)
          .map_or(Value::Null, |v| v.try_into().unwrap_or(Value::Null))
      })
      .collect(),
    PreUpdateCase::Delete(accessor) => (0..accessor.get_column_count())
      .map(|idx| -> Value {
        accessor
          .get_old_column_value(idx)
          .map_or(Value::Null, |v| v.try_into().unwrap_or(Value::Null))
      })
      .collect(),
    PreUpdateCase::Update {
      new_value_accessor: accessor,
      ..
    } => (0..accessor.get_column_count())
      .map(|idx| -> Value {
        accessor
          .get_new_column_value(idx)
          .map_or(Value::Null, |v| v.try_into().unwrap_or(Value::Null))
      })
      .collect(),
    PreUpdateCase::Unknown => {
      return None;
    }
  });
}

pub fn list_databases(conn: &impl SyncConnectionTrait) -> Result<Vec<Database>, Error> {
  let rows = conn.query_rows("SELECT seq, name FROM pragma_database_list", ())?;

  return rows
    .iter()
    .map(|row| {
      return Ok(Database {
        seq: row.get(0)?,
        name: row.get(1)?,
      });
    })
    .collect::<Result<Vec<_>, _>>();
}
