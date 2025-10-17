use trailbase_schema::json::{JsonError, value_to_flat_json};
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

fn row_to_flat_json_array(row: &Row) -> Result<Vec<serde_json::Value>, JsonError> {
  return (0..row.column_count())
    .map(|i| -> Result<serde_json::Value, JsonError> {
      let value = row.get_value(i).ok_or(JsonError::ValueNotFound)?;
      return value_to_flat_json(value);
    })
    .collect();
}

#[inline]
pub(crate) fn rows_to_flat_json_arrays(
  rows: &Rows,
) -> Result<Vec<Vec<serde_json::Value>>, JsonError> {
  return rows.iter().map(row_to_flat_json_array).collect();
}
