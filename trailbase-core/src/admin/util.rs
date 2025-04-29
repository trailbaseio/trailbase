use trailbase_schema::sqlite::{Column, ColumnDataType};
use trailbase_sqlite::ValueType;
use trailbase_sqlite::rows::Rows;

/// Best-effort conversion from row values to column definition.
///
/// WARN: This is lossy and whenever possible we should rely on parsed "CREATE TABLE" statement for
/// the respective column.
pub(crate) fn rows_to_columns(rows: &Rows) -> Vec<Column> {
  let mut columns: Vec<Column> = vec![];
  for i in 0..rows.column_count() {
    columns.push(Column {
      name: rows.column_name(i).unwrap_or("<missing>").to_string(),
      data_type: match rows.column_type(i).unwrap_or(ValueType::Null) {
        ValueType::Real => ColumnDataType::Real,
        ValueType::Text => ColumnDataType::Text,
        ValueType::Integer => ColumnDataType::Integer,
        ValueType::Null => ColumnDataType::Null,
        ValueType::Blob => ColumnDataType::Blob,
      },
      // We cannot derive the options from a row of data.
      options: vec![],
    });
  }

  return columns;
}
