use regex::Regex;
use trailbase_qs::{Combiner, CompareOp};
use trailbase_schema::sqlite::{Column, ColumnDataType};

use crate::records::RecordError;

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnOpValue {
  pub column: String,
  pub op: CompareOp,
  pub value: rusqlite::types::Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ValueOrComposite {
  Value(ColumnOpValue),
  Composite(Combiner, Vec<ValueOrComposite>),
}

pub enum Filter {
  Passthrough,
  Record(ValueOrComposite),
}

fn any_qs_value_to_sql(value: trailbase_qs::Value) -> rusqlite::types::Value {
  use base64::prelude::*;
  use rusqlite::types::Value;
  use trailbase_qs::Value as QsValue;

  return match value {
    QsValue::String(s) => {
      if let Ok(b) = BASE64_URL_SAFE.decode(&s) {
        Value::Blob(b)
      } else {
        Value::Text(s.clone())
      }
    }
    QsValue::Integer(i) => Value::Integer(i),
    QsValue::Double(d) => Value::Real(d),
  };
}

pub(crate) fn qs_value_to_sql_with_constraints(
  column: &Column,
  value: trailbase_qs::Value,
) -> Result<rusqlite::types::Value, RecordError> {
  use base64::prelude::*;
  use rusqlite::types::Value;
  use trailbase_qs::Value as QsValue;

  return match column.data_type {
    ColumnDataType::Null => Err(RecordError::BadRequest("Invalid query")),
    ColumnDataType::Any => Ok(any_qs_value_to_sql(value)),
    ColumnDataType::Blob => match value {
      QsValue::String(s) => Ok(Value::Blob(
        BASE64_URL_SAFE
          .decode(&s)
          .map_err(|_err| RecordError::BadRequest("Invalid query"))?,
      )),
      _ => Err(RecordError::BadRequest("Invalid query")),
    },
    ColumnDataType::Text => match value {
      QsValue::String(s) => Ok(Value::Text(s)),
      QsValue::Integer(i) => Ok(Value::Text(i.to_string())),
      QsValue::Double(d) => Ok(Value::Text(d.to_string())),
    },
    ColumnDataType::Integer | ColumnDataType::Int => match value {
      QsValue::Integer(i) => Ok(Value::Integer(i)),
      _ => Err(RecordError::BadRequest("Invalid query")),
    },
    ColumnDataType::Real => match value {
      QsValue::Integer(i) => Ok(Value::Real(i as f64)),
      QsValue::Double(d) => Ok(Value::Real(d)),
      _ => Err(RecordError::BadRequest("Invalid query")),
    },
    ColumnDataType::Numeric => match value {
      QsValue::Integer(i) => Ok(Value::Integer(i)),
      QsValue::Double(d) => Ok(Value::Real(d)),
      _ => Err(RecordError::BadRequest("Invalid query")),
    }, // not allowed in strict mode.
    _ => Err(RecordError::BadRequest("Invalid query")),
  };
}

pub(crate) fn qs_filter_to_record_filter(
  columns: &[Column],
  filter: trailbase_qs::ValueOrComposite,
) -> Result<ValueOrComposite, RecordError> {
  return match filter {
    trailbase_qs::ValueOrComposite::Value(col_op_value) => {
      let column = columns
        .iter()
        .find(|c| c.name == col_op_value.column)
        .ok_or_else(|| RecordError::BadRequest("Invalid query"))?;

      Ok(ValueOrComposite::Value(ColumnOpValue {
        column: col_op_value.column,
        op: col_op_value.op,
        value: qs_value_to_sql_with_constraints(column, col_op_value.value)?,
      }))
    }
    trailbase_qs::ValueOrComposite::Composite(combiner, expressions) => {
      Ok(ValueOrComposite::Composite(
        combiner,
        expressions
          .into_iter()
          .map(|value_or_composite| qs_filter_to_record_filter(columns, value_or_composite))
          .collect::<Result<Vec<_>, _>>()?,
      ))
    }
  };
}

#[inline]
fn compare_values(
  op: &CompareOp,
  record_value: &rusqlite::types::Value,
  filter_value: &rusqlite::types::Value,
) -> bool {
  use rusqlite::types::Value;

  return match op {
    CompareOp::Equal => *record_value == *filter_value,
    CompareOp::NotEqual => *record_value != *filter_value,
    CompareOp::GreaterThanEqual => match (record_value, filter_value) {
      (Value::Null, Value::Null) => false,
      (Value::Integer(a), Value::Integer(b)) => a >= b,
      (Value::Real(a), Value::Real(b)) => a >= b,
      (Value::Real(a), Value::Integer(b)) => *a >= *b as f64,
      (Value::Text(a), Value::Text(b)) => a >= b,
      (Value::Blob(a), Value::Blob(b)) => a >= b,
      _ => false,
    },
    CompareOp::GreaterThan => match (record_value, filter_value) {
      (Value::Null, Value::Null) => false,
      (Value::Integer(a), Value::Integer(b)) => a > b,
      (Value::Real(a), Value::Real(b)) => a > b,
      (Value::Real(a), Value::Integer(b)) => *a > *b as f64,
      (Value::Text(a), Value::Text(b)) => a > b,
      (Value::Blob(a), Value::Blob(b)) => a > b,
      _ => false,
    },
    CompareOp::LessThanEqual => match (record_value, filter_value) {
      (Value::Null, Value::Null) => false,
      (Value::Integer(a), Value::Integer(b)) => a <= b,
      (Value::Real(a), Value::Real(b)) => a <= b,
      (Value::Real(a), Value::Integer(b)) => *a <= *b as f64,
      (Value::Text(a), Value::Text(b)) => a <= b,
      (Value::Blob(a), Value::Blob(b)) => a <= b,
      _ => false,
    },
    CompareOp::LessThan => match (record_value, filter_value) {
      (Value::Null, Value::Null) => false,
      (Value::Integer(a), Value::Integer(b)) => a < b,
      (Value::Real(a), Value::Real(b)) => a < b,
      (Value::Real(a), Value::Integer(b)) => *a < *b as f64,
      (Value::Text(a), Value::Text(b)) => a < b,
      (Value::Blob(a), Value::Blob(b)) => a < b,
      _ => false,
    },
    CompareOp::Is => match filter_value {
      Value::Text(s) if s == "NULL" => matches!(record_value, Value::Null),
      Value::Text(s) if s == "!NULL" => !matches!(record_value, Value::Null),
      _ => false,
    },
    CompareOp::Regexp => match (record_value, filter_value) {
      (Value::Text(record), Value::Text(filter)) => {
        Regex::new(filter).is_ok_and(|re| re.is_match(record))
      }
      _ => false,
    },
    CompareOp::Like => match (record_value, filter_value) {
      (Value::Text(record), Value::Text(filter)) => {
        sql_like_to_regex(filter).is_ok_and(|re| re.is_match(record))
      }
      _ => false,
    },
  };
}

pub(crate) fn apply_filter_recursively_to_record(
  filter: &ValueOrComposite,
  record: &indexmap::IndexMap<&str, rusqlite::types::Value>,
) -> bool {
  return match filter {
    ValueOrComposite::Value(col_op_value) => {
      let ColumnOpValue {
        column,
        op,
        value: filter_value,
      } = col_op_value;

      record
        .get(column.as_str())
        .is_some_and(|record_value| compare_values(op, record_value, filter_value))
    }
    ValueOrComposite::Composite(combiner, expressions) => match combiner {
      Combiner::And => {
        for expr in expressions {
          if !(apply_filter_recursively_to_record(expr, record)) {
            return false;
          }
        }
        true
      }
      Combiner::Or => {
        for expr in expressions {
          if apply_filter_recursively_to_record(expr, record) {
            return true;
          }
        }
        false
      }
    },
  };
}

fn sql_like_to_regex(like: &'_ str) -> Result<Regex, regex::Error> {
  let mut re = String::with_capacity(2 * like.len());

  let mut prev: Option<char> = None;
  for c in like.chars() {
    match c {
      '%' => {
        if prev == Some('\\') {
          re.push('%');
        } else {
          re.push_str(".*");
        }
      }
      '_' => {
        if prev == Some('\\') {
          re.push('_');
        } else {
          re.push('.');
        }
      }
      '\\' => {
        if prev == Some('\\') {
          re.push_str(r"\\");
          prev = None;
          continue;
        }
      }
      c => {
        re.push(c);
      }
    }

    prev = Some(c);
  }

  return Regex::new(&re);
}

#[cfg(test)]
mod tests {
  use super::*;

  use indexmap::IndexMap;
  use rusqlite::types::Value;

  #[test]
  fn test_sql_like_to_regex() {
    assert_eq!(".*abc.*", sql_like_to_regex("%abc%").unwrap().as_str());
    assert_eq!(".a.bc.*", sql_like_to_regex("_a_bc%").unwrap().as_str());
    assert_eq!("%_", sql_like_to_regex("\\%\\_").unwrap().as_str());
    assert_eq!(r"\\.*", sql_like_to_regex(r"\\%").unwrap().as_str());
  }

  #[test]
  fn test_basic_value_filter() {
    let record: IndexMap<&str, Value> = IndexMap::from([("a", Value::Text("a value".to_string()))]);

    assert!(apply_filter_recursively_to_record(
      &ValueOrComposite::Value(ColumnOpValue {
        column: "a".to_string(),
        op: CompareOp::Equal,
        value: Value::Text("a value".to_string()),
      }),
      &record
    ));

    assert!(!apply_filter_recursively_to_record(
      &ValueOrComposite::Value(ColumnOpValue {
        column: "a".to_string(),
        op: CompareOp::NotEqual,
        value: Value::Text("a value".to_string()),
      }),
      &record
    ));

    assert!(apply_filter_recursively_to_record(
      &ValueOrComposite::Value(ColumnOpValue {
        column: "a".to_string(),
        op: CompareOp::LessThanEqual,
        value: Value::Text("a value".to_string()),
      }),
      &record
    ));

    assert!(!apply_filter_recursively_to_record(
      &ValueOrComposite::Value(ColumnOpValue {
        column: "a".to_string(),
        op: CompareOp::LessThan,
        value: Value::Text("a value".to_string()),
      }),
      &record
    ));
  }

  #[test]
  fn test_basic_composite_filter() {
    let record: IndexMap<&str, Value> =
      IndexMap::from([("a", Value::Integer(5)), ("b", Value::Integer(-5))]);

    assert!(apply_filter_recursively_to_record(
      &ValueOrComposite::Composite(
        Combiner::And,
        vec![
          ValueOrComposite::Value(ColumnOpValue {
            column: "a".to_string(),
            op: CompareOp::Equal,
            value: Value::Integer(5),
          }),
          ValueOrComposite::Value(ColumnOpValue {
            column: "b".to_string(),
            op: CompareOp::LessThan,
            value: Value::Integer(-2),
          }),
        ]
      ),
      &record
    ));

    assert!(!apply_filter_recursively_to_record(
      &ValueOrComposite::Composite(
        Combiner::And,
        vec![
          ValueOrComposite::Value(ColumnOpValue {
            column: "a".to_string(),
            op: CompareOp::Equal,
            value: Value::Integer(5),
          }),
          ValueOrComposite::Value(ColumnOpValue {
            column: "b".to_string(),
            op: CompareOp::GreaterThanEqual,
            value: Value::Integer(-2),
          }),
        ]
      ),
      &record
    ));
  }
}
