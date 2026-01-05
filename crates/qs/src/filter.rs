use itertools::Itertools;
/// Custom deserializers for urlencoded filters.
///
/// Supported examples:
///
/// filters[column]=value  <-- might be nice otherwise below.
/// filter[column]=value
/// filters[column][eq]=value
/// filters[and][0][column0][eq]=value0&filters[and][1][column1][eq]=value1
/// filters[and][0][or][0][column0]=value0&[and][0][or][1][column1]=value1
use std::collections::BTreeMap;

use crate::column_rel_value::{ColumnOpValue, CompareOp, serde_value_to_single_column_rel_value};
use crate::value::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Combiner {
  And,
  Or,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ValueOrComposite {
  Value(ColumnOpValue),
  Composite(Combiner, Vec<ValueOrComposite>),
}

impl ValueOrComposite {
  /// Returns SQL query, and a list of (param_name, param_value).
  pub fn into_sql<V, E>(
    self,
    column_prefix: Option<&str>,
    convert: &dyn Fn(&str, Value) -> Result<V, E>,
  ) -> Result<(String, Vec<(String, V)>), E> {
    fn render_value<V, E>(
      column_op_value: ColumnOpValue,
      column_prefix: Option<&str>,
      convert: &dyn Fn(&str, Value) -> Result<V, E>,
      index: &mut usize,
    ) -> Result<(String, Option<(String, V)>), E> {
      let v = column_op_value.value;
      let c = column_op_value.column;

      return match column_op_value.op {
        CompareOp::Is => {
          debug_assert!(matches!(v, Value::String(_)), "{v:?}");

          Ok(match column_prefix {
            Some(p) => (format!(r#"{p}."{c}" IS {v}"#), None),
            None => (format!(r#""{c}" IS {v}"#), None),
          })
        }
        op => {
          let param = param_name(*index);
          *index += 1;

          Ok(match column_prefix {
            Some(p) => (
              format!(r#"{p}."{c}" {o} {param}"#, o = op.as_sql()),
              Some((param, convert(&c, v)?)),
            ),
            None => (
              format!(r#""{c}" {o} {param}"#, o = op.as_sql()),
              Some((param, convert(&c, v)?)),
            ),
          })
        }
      };
    }

    fn recurse<V, E>(
      v: ValueOrComposite,
      column_prefix: Option<&str>,
      convert: &dyn Fn(&str, Value) -> Result<V, E>,
      index: &mut usize,
    ) -> Result<(String, Vec<(String, V)>), E> {
      match v {
        ValueOrComposite::Value(v) => {
          return Ok(match render_value(v, column_prefix, convert, index)? {
            (sql, Some(param)) => (sql, vec![param]),
            (sql, None) => (sql, vec![]),
          });
        }
        ValueOrComposite::Composite(combiner, vec) => {
          let mut fragments = Vec::<String>::with_capacity(vec.len());
          let mut params = Vec::<(String, V)>::with_capacity(vec.len());

          for value_or_composite in vec {
            let (f, p) = recurse(value_or_composite, column_prefix, convert, index)?;
            fragments.push(f);
            params.extend(p);
          }

          let fragment = format!(
            "({})",
            fragments.join(match combiner {
              Combiner::And => " AND ",
              Combiner::Or => " OR ",
            }),
          );
          return Ok((fragment, params));
        }
      };
    }

    let mut index: usize = 0;
    return recurse(self, column_prefix, convert, &mut index);
  }

  /// Return a query-string fragment for this filter (no leading '&').
  pub fn to_query(&self) -> String {
    /// Return a (key, value) pair suitable for query-string serialization (not percent-encoded).
    fn render_value(prefix: &str, v: &ColumnOpValue) -> String {
      let value: std::borrow::Cow<str> = match (&v.op, &v.value) {
        (CompareOp::Is, Value::String(s)) if s == "NOT NULL" => "!NULL".into(),
        (CompareOp::Is, Value::String(s)) if s == "NULL" => "NULL".into(),
        (_, Value::String(s)) => s.into(),
        (_, Value::Integer(i)) => i.to_string().into(),
        (_, Value::Double(d)) => d.to_string().into(),
      };

      let column = &v.column;
      return if matches!(v.op, CompareOp::Equal) {
        format!("{prefix}[{column}]={value}")
      } else {
        format!("{prefix}[{column}][{}]={value}", v.op.as_query())
      };
    }

    fn recurse(v: &ValueOrComposite, prefix: &str) -> Vec<String> {
      return match v {
        ValueOrComposite::Value(v) => vec![render_value(prefix, v)],
        ValueOrComposite::Composite(combiner, vec) => {
          let comb = match combiner {
            Combiner::And => "$and",
            Combiner::Or => "$or",
          };

          vec
            .iter()
            .enumerate()
            .flat_map(|(i, el)| recurse(el, &format!("{}[{}][{}]", prefix, comb, i)))
            .collect()
        }
      };
    }

    return recurse(self, "filter").into_iter().join("&");
  }
}

fn serde_value_to_value_or_composite<'de, D>(
  value: serde_value::Value,
  depth: usize,
) -> Result<ValueOrComposite, D::Error>
where
  D: serde::de::Deserializer<'de>,
{
  use serde::de::Error;
  use serde_value::Value;

  // Limit recursion depth
  if depth >= 5 {
    return Err(Error::custom("Recursion limit exceeded"));
  }

  // We always expect [key] = value, i.e. a Map[key] = value.
  let Value::Map(mut m) = value else {
    return Err(Error::invalid_type(
      crate::util::unexpected(&value),
      &"[($and|$or)][index][<nested>] or [column_name][$op]=value",
    ));
  };

  return match m.len() {
    0 => Ok(ValueOrComposite::Composite(Combiner::And, vec![])),
    1 => {
      let first = m.pop_first().expect("len == 1");
      let (Value::String(key), v) = first else {
        return Err(Error::invalid_type(
          crate::util::unexpected(&first.0),
          &"String",
        ));
      };

      match (key.as_str(), v) {
        // Recursive cases.
        ("$and", Value::Seq(values)) => combine::<D>(Combiner::And, values, depth),
        ("$and", v) => Err(Error::invalid_type(
          crate::util::unexpected(&v),
          &"sequence",
        )),
        ("$or", Value::Seq(values)) => combine::<D>(Combiner::Or, values, depth),
        ("$or", v) => Err(Error::invalid_type(
          crate::util::unexpected(&v),
          &"sequence",
        )),
        // Single column_name but multiple values, i.e. multiple filters on the same col.
        (col_name, Value::Map(m)) if m.len() > 1 => Ok(ValueOrComposite::Composite(
          Combiner::And,
          m.into_iter()
            .map(|(key, value)| {
              return Ok(ValueOrComposite::Value(
                serde_value_to_single_column_rel_value::<D>(
                  col_name.to_string(),
                  Value::Map(BTreeMap::from([(key, value)])),
                )?,
              ));
            })
            .collect::<Result<Vec<_>, _>>()?,
        )),
        // For any other string-type key, turn into a single value.
        (_key, v) => Ok(ValueOrComposite::Value(
          serde_value_to_single_column_rel_value::<D>(key, v)?,
        )),
      }
    }
    // Multiple different keys on the same same level, i.e. no explicit grouping by a single key
    // like "$and" or "$or" => Implicit AND composite.
    _n => combine::<D>(
      Combiner::And,
      m.into_iter()
        .map(|(k, v)| Value::Map(BTreeMap::from([(k, v)]))),
      depth,
    ),
  };
}

/// Recursively combine nested filter expressions.
fn combine<'de, D>(
  combiner: Combiner,
  values: impl IntoIterator<Item = serde_value::Value>,
  depth: usize,
) -> Result<ValueOrComposite, D::Error>
where
  D: serde::de::Deserializer<'de>,
{
  return Ok(ValueOrComposite::Composite(
    combiner,
    values
      .into_iter()
      .map(|v| serde_value_to_value_or_composite::<D>(v, depth + 1))
      .collect::<Result<Vec<_>, _>>()?,
  ));
}

impl<'de> serde::de::Deserialize<'de> for ValueOrComposite {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::de::Deserializer<'de>,
  {
    return serde_value_to_value_or_composite::<D>(
      serde_value::Value::deserialize(deserializer)?,
      0,
    );
  }
}

#[inline]
fn param_name(index: usize) -> String {
  let mut s = String::with_capacity(10);
  s.push_str(":__p");
  s.push_str(&index.to_string());
  return s;
}

#[cfg(test)]
mod tests {
  use super::*;

  use rusqlite::types::Value as SqlValue;
  use serde_qs::Config;

  use crate::column_rel_value::{ColumnOpValue, CompareOp};
  use crate::query::FilterQuery as Query;
  use crate::value::Value;

  #[test]
  fn test_filter_parsing() {
    let qs = Config::new(5, false);

    let m_empty: Query = qs.deserialize_str("").unwrap();
    assert_eq!(m_empty.filter, None);

    let q0 = "filter[$and][0][col0]=val0";
    let f0 = qs.deserialize_str::<Query>(q0).unwrap().filter.unwrap();
    assert_eq!(
      f0,
      ValueOrComposite::Composite(
        Combiner::And,
        vec![ValueOrComposite::Value(ColumnOpValue {
          column: "col0".to_string(),
          op: CompareOp::Equal,
          value: Value::String("val0".to_string()),
        })]
      ),
    );
    assert_eq!(q0, f0.to_query());

    let q1 = "filter[$and][0][col0]=val0&filter[$and][1][col1]=val1";
    let f1 = qs.deserialize_str::<Query>(q1).unwrap().filter.unwrap();
    assert_eq!(
      f1,
      ValueOrComposite::Composite(
        Combiner::And,
        vec![
          ValueOrComposite::Value(ColumnOpValue {
            column: "col0".to_string(),
            op: CompareOp::Equal,
            value: Value::String("val0".to_string()),
          }),
          ValueOrComposite::Value(ColumnOpValue {
            column: "col1".to_string(),
            op: CompareOp::Equal,
            value: Value::String("val1".to_string()),
          }),
        ]
      )
    );
    assert_eq!(q1, f1.to_query());

    let q3 = "filter[col0][$is]=!NULL";
    let f3 = qs.deserialize_str::<Query>(q3).unwrap().filter.unwrap();
    assert_eq!(
      f3,
      ValueOrComposite::Value(ColumnOpValue {
        column: "col0".to_string(),
        op: CompareOp::Is,
        value: Value::String("NOT NULL".to_string()),
      })
    );
    assert_eq!(q3, f3.to_query());

    // Combiners with only one element each.
    let m = qs.deserialize_str::<Query>("filter[$and][0][col0]=val0&filter[$or][1][col1]=val1");
    assert!(m.is_ok(), "M: {m:?}");

    // test implicit $and.
    let q2 = "filter[col0]=val0&filter[col1]=val1";
    let f2 = qs.deserialize_str::<Query>(q2).unwrap().filter.unwrap();

    assert_eq!(
      f2,
      ValueOrComposite::Composite(
        Combiner::And,
        vec![
          ValueOrComposite::Value(ColumnOpValue {
            column: "col0".to_string(),
            op: CompareOp::Equal,
            value: Value::String("val0".to_string()),
          }),
          ValueOrComposite::Value(ColumnOpValue {
            column: "col1".to_string(),
            op: CompareOp::Equal,
            value: Value::String("val1".to_string()),
          }),
        ]
      )
    );

    let q2_explicit = "filter[$and][0][col0]=val0&filter[$and][1][col1]=val1";
    let f2_explicit = qs
      .deserialize_str::<Query>(q2_explicit)
      .unwrap()
      .filter
      .unwrap();

    assert_eq!(f2_explicit, f2);
    assert_eq!(q2_explicit, f2.to_query());
  }

  #[test]
  fn test_filter_to_sql() {
    let v0 = ValueOrComposite::Value(ColumnOpValue {
      column: "col0".to_string(),
      op: CompareOp::Equal,
      value: Value::String("val0".to_string()),
    });

    let convert = |_: &str, value: Value| -> Result<SqlValue, String> {
      return Ok(match value {
        Value::String(s) => SqlValue::Text(s),
        Value::Integer(i) => SqlValue::Integer(i),
        Value::Double(d) => SqlValue::Real(d),
      });
    };

    let sql0 = v0
      .clone()
      .into_sql(/* column_prefix= */ None, &convert)
      .unwrap();
    assert_eq!(sql0.0, r#""col0" = :__p0"#);
    let sql0 = v0
      .into_sql(/* column_prefix= */ Some("p"), &convert)
      .unwrap();
    assert_eq!(sql0.0, r#"p."col0" = :__p0"#);

    let v1 = ValueOrComposite::Value(ColumnOpValue {
      column: "col0".to_string(),
      op: CompareOp::Is,
      value: Value::String("NULL".to_string()),
    });
    let sql1 = v1.into_sql(None, &convert).unwrap();
    assert_eq!(sql1.0, r#""col0" IS NULL"#, "{sql1:?}",);
  }
}
