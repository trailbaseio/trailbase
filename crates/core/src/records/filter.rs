use trailbase_qs::{Combiner, CompareOp};

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

pub(crate) fn qs_value_to_sql(value: trailbase_qs::Value) -> rusqlite::types::Value {
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
    QsValue::Bool(b) => Value::Integer(if b { 1 } else { 0 }),
  };
}

pub(crate) fn qs_filter_to_record_filter(
  filter: trailbase_qs::ValueOrComposite,
) -> ValueOrComposite {
  return match filter {
    trailbase_qs::ValueOrComposite::Value(col_op_value) => ValueOrComposite::Value(ColumnOpValue {
      column: col_op_value.column,
      op: col_op_value.op,
      value: qs_value_to_sql(col_op_value.value),
    }),
    trailbase_qs::ValueOrComposite::Composite(combiner, expressions) => {
      ValueOrComposite::Composite(
        combiner,
        expressions
          .into_iter()
          .map(qs_filter_to_record_filter)
          .collect(),
      )
    }
  };
}

fn compare_values(
  op: &CompareOp,
  record_value: &rusqlite::types::Value,
  filter_value: &rusqlite::types::Value,
) -> Result<bool, String> {
  use rusqlite::types::Value;

  return match op {
    CompareOp::Equal => Ok(*record_value == *filter_value),
    CompareOp::NotEqual => Ok(*record_value != *filter_value),
    CompareOp::GreaterThanEqual => match (record_value, filter_value) {
      (Value::Null, Value::Null) => Ok(false),
      (Value::Integer(a), Value::Integer(b)) => Ok(a >= b),
      (Value::Real(a), Value::Real(b)) => Ok(a >= b),
      (Value::Text(a), Value::Text(b)) => Ok(a >= b),
      (Value::Blob(a), Value::Blob(b)) => Ok(a >= b),
      _ => Ok(false),
    },
    CompareOp::GreaterThan => match (record_value, filter_value) {
      (Value::Null, Value::Null) => Ok(false),
      (Value::Integer(a), Value::Integer(b)) => Ok(a > b),
      (Value::Real(a), Value::Real(b)) => Ok(a > b),
      (Value::Text(a), Value::Text(b)) => Ok(a > b),
      (Value::Blob(a), Value::Blob(b)) => Ok(a > b),
      _ => Ok(false),
    },
    CompareOp::LessThanEqual => match (record_value, filter_value) {
      (Value::Null, Value::Null) => Ok(false),
      (Value::Integer(a), Value::Integer(b)) => Ok(a <= b),
      (Value::Real(a), Value::Real(b)) => Ok(a <= b),
      (Value::Text(a), Value::Text(b)) => Ok(a <= b),
      (Value::Blob(a), Value::Blob(b)) => Ok(a <= b),
      _ => Ok(false),
    },
    CompareOp::LessThan => match (record_value, filter_value) {
      (Value::Null, Value::Null) => Ok(false),
      (Value::Integer(a), Value::Integer(b)) => Ok(a < b),
      (Value::Real(a), Value::Real(b)) => Ok(a < b),
      (Value::Text(a), Value::Text(b)) => Ok(a < b),
      (Value::Blob(a), Value::Blob(b)) => Ok(a < b),
      _ => Ok(false),
    },
    CompareOp::Is => match filter_value {
      Value::Text(s) if s == "NULL" => Ok(matches!(record_value, Value::Null)),
      Value::Text(s) if s == "!NULL" => Ok(!matches!(record_value, Value::Null)),
      _ => Ok(false),
    },
    CompareOp::Regexp => match (record_value, filter_value) {
      (Value::Text(record), Value::Text(filter)) => {
        if let Ok(re) = regex::Regex::new(filter) {
          Ok(re.is_match(record))
        } else {
          Ok(false)
        }
      }
      _ => Ok(false),
    },
    CompareOp::Like => Err("not implemented".into()),
  };
}

pub(crate) fn apply_filter_to_record(
  filter: &ValueOrComposite,
  record: &indexmap::IndexMap<&str, rusqlite::types::Value>,
) -> Result<bool, String> {
  match filter {
    ValueOrComposite::Value(col_op_value) => {
      let ColumnOpValue {
        column,
        op,
        value: filter_value,
      } = col_op_value;

      let Some(record_value) = record.get(column.as_str()) else {
        // Missing value.
        return Ok(false);
      };

      return compare_values(op, record_value, filter_value);
    }
    ValueOrComposite::Composite(combiner, expressions) => match combiner {
      Combiner::And => {
        for expr in expressions {
          if !(apply_filter_to_record(expr, record)?) {
            return Ok(false);
          }
        }
        return Ok(true);
      }
      Combiner::Or => {
        for expr in expressions {
          if apply_filter_to_record(expr, record)? {
            return Ok(true);
          }
        }
        return Ok(false);
      }
    },
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  use indexmap::IndexMap;
  use rusqlite::types::Value;

  #[test]
  fn test_basic_value_filter() {
    let record: IndexMap<&str, Value> = IndexMap::from([("a", Value::Text("a value".to_string()))]);

    assert!(
      apply_filter_to_record(
        &ValueOrComposite::Value(ColumnOpValue {
          column: "a".to_string(),
          op: CompareOp::Equal,
          value: Value::Text("a value".to_string()),
        }),
        &record
      )
      .unwrap()
    );

    assert!(
      !apply_filter_to_record(
        &ValueOrComposite::Value(ColumnOpValue {
          column: "a".to_string(),
          op: CompareOp::NotEqual,
          value: Value::Text("a value".to_string()),
        }),
        &record
      )
      .unwrap()
    );

    assert!(
      apply_filter_to_record(
        &ValueOrComposite::Value(ColumnOpValue {
          column: "a".to_string(),
          op: CompareOp::LessThanEqual,
          value: Value::Text("a value".to_string()),
        }),
        &record
      )
      .unwrap()
    );

    assert!(
      !apply_filter_to_record(
        &ValueOrComposite::Value(ColumnOpValue {
          column: "a".to_string(),
          op: CompareOp::LessThan,
          value: Value::Text("a value".to_string()),
        }),
        &record
      )
      .unwrap()
    );
  }

  #[test]
  fn test_basic_composite_filter() {
    let record: IndexMap<&str, Value> =
      IndexMap::from([("a", Value::Integer(5)), ("b", Value::Integer(-5))]);

    assert!(
      apply_filter_to_record(
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
      )
      .unwrap()
    );

    assert!(
      !apply_filter_to_record(
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
      )
      .unwrap()
    );
  }
}
