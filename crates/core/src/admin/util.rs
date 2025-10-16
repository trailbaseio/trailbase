use trailbase_common::SqlValue;
use trailbase_schema::json::JsonError;
use trailbase_schema::sqlite::{Column, ColumnAffinityType, ColumnDataType};
use trailbase_sqlite::{Row, Rows, ValueType};

/// Best-effort conversion from row values to column definition.
///
/// WARN: This is lossy and whenever possible we should rely on parsed "CREATE TABLE" statement for
/// the respective column.
pub(crate) fn rows_to_columns(rows: &Rows) -> Vec<Column> {
  return (0..rows.column_count())
    .map(|i| {
      let data_type = match rows.column_type(i).unwrap_or(ValueType::Null) {
        ValueType::Null => ColumnDataType::Any,
        ValueType::Real => ColumnDataType::Real,
        ValueType::Text => ColumnDataType::Text,
        ValueType::Integer => ColumnDataType::Integer,
        ValueType::Blob => ColumnDataType::Blob,
      };

      return Column {
        name: rows.column_name(i).unwrap_or("<missing>").to_string(),
        type_name: "".to_string(),
        data_type,
        affinity_type: ColumnAffinityType::from_data_type(data_type),
        // We cannot derive the options from a row of data.
        options: vec![],
      };
    })
    .collect();
}

fn row_to_sql_value_row(row: &Row) -> Result<Vec<SqlValue>, JsonError> {
  return (0..row.column_count())
    .map(|i| -> Result<SqlValue, JsonError> {
      let value = row.get_value(i).ok_or(JsonError::ValueNotFound)?;
      return Ok(value.into());
    })
    .collect();
}

// TODO: We should use a different error types - no JSON at play here.
#[inline]
pub(crate) fn rows_to_sql_value_rows(rows: &Rows) -> Result<Vec<Vec<SqlValue>>, JsonError> {
  return rows.iter().map(row_to_sql_value_row).collect();
}
