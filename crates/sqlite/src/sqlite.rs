use rusqlite::hooks::PreUpdateCase;

use crate::connection::Database;
use crate::error::Error;
use crate::value::Value;

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

pub fn list_databases(conn: &rusqlite::Connection) -> Result<Vec<Database>, Error> {
  let mut stmt = conn.prepare("SELECT seq, name FROM pragma_database_list")?;
  let mut rows = stmt.raw_query();

  let mut databases: Vec<Database> = vec![];
  while let Some(row) = rows.next()? {
    databases.push(serde_rusqlite::from_row(row).map_err(Error::DeserializeValue)?)
  }

  return Ok(databases);
}
