/// Custom deserializers for urlencoded filters.
///
/// Supported examples:
///
/// filters[column]=value  <-- might be nice otherwise below.
/// filter[column]=value
/// filters[column][eq]=value
/// filters[and][0][column0][eq]=value0&filters[and][1][column1][eq]=value1
/// filters[and][0][or][0][column0]=value0&[and][0][or][1][column1]=value1
use rusqlite::types::Value as SqlValue;
use std::collections::BTreeMap;

use crate::column_rel_value::{ColumnOpValue, serde_value_to_single_column_rel_value};
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
  pub fn into_sql<E>(
    self,
    column_prefix: Option<&str>,
    convert: &dyn Fn(&str, Value) -> Result<SqlValue, E>,
  ) -> Result<(String, Vec<(String, SqlValue)>), E> {
    let mut index: usize = 0;
    return self.into_sql_impl(column_prefix, convert, &mut index);
  }

  fn into_sql_impl<E>(
    self,
    column_prefix: Option<&str>,
    convert: &dyn Fn(&str, Value) -> Result<SqlValue, E>,
    index: &mut usize,
  ) -> Result<(String, Vec<(String, SqlValue)>), E> {
    match self {
      Self::Value(v) => {
        return Ok(match v.into_sql(column_prefix, convert, index)? {
          (sql, Some(param)) => (sql, vec![param]),
          (sql, None) => (sql, vec![]),
        });
      }
      Self::Composite(combiner, vec) => {
        let mut fragments = Vec::<String>::with_capacity(vec.len());
        let mut params = Vec::<(String, SqlValue)>::with_capacity(vec.len());

        for value_or_composite in vec {
          let (f, p) = value_or_composite.into_sql_impl::<E>(column_prefix, convert, index)?;
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

  pub(crate) fn into_qs_impl(&self, prefix: &str) -> Vec<(String, String)> {
    match self {
      ValueOrComposite::Value(colop) => {
        let (key, value) = colop.into_qs(prefix);
        return vec![(key, value)];
      }
      ValueOrComposite::Composite(combiner, vec) => {
        let comb = match combiner {
          Combiner::And => "$and",
          Combiner::Or => "$or",
        };

        let mut out_vec = Vec::new();
        for (i, child) in vec.iter().enumerate() {
          // prefix[$and][0] ...
          let child_prefix = format!("{}[{}][{}]", prefix, comb, i);
          out_vec.extend(child.into_qs_impl(&child_prefix));
        }

        return out_vec;
      }
    }
  }

  /// Return a query-string fragment for this filter (no leading '&').
  pub fn into_qs(&self) -> String {
    let pairs = self.into_qs_impl("filter");
    return pairs
      .into_iter()
      .map(|(k, v)| format!("{}={}", k, v))
      .collect::<Vec<_>>()
      .join("&");
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

#[cfg(test)]
mod tests {
  use super::*;

  use serde::Deserialize;
  use serde_qs::Config;

  use crate::column_rel_value::{ColumnOpValue, CompareOp};
  use crate::value::Value;

  #[derive(Clone, Debug, Default, Deserialize)]
  struct Query {
    filter: Option<ValueOrComposite>,
  }

  #[test]
  fn test_filter_parsing() {
    let qs = Config::new(5, false);

    let m_empty: Query = qs.deserialize_str("").unwrap();
    assert_eq!(m_empty.filter, None);

    let m0: Query = qs.deserialize_str("filter[$and][0][col0]=val0").unwrap();
    assert_eq!(
      m0.filter.unwrap(),
      ValueOrComposite::Composite(
        Combiner::And,
        vec![ValueOrComposite::Value(ColumnOpValue {
          column: "col0".to_string(),
          op: CompareOp::Equal,
          value: Value::String("val0".to_string()),
        })]
      ),
    );

    let m1: Query = qs
      .deserialize_str("filter[$and][0][col0]=val0&filter[$and][1][col1]=val1")
      .unwrap();
    assert_eq!(
      m1.filter.unwrap(),
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

    let m3: Query = qs.deserialize_str("filter[col0][$is]=!NULL").unwrap();
    assert_eq!(
      m3.filter.unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "col0".to_string(),
        op: CompareOp::Is,
        value: Value::String("NOT NULL".to_string()),
      })
    );

    let m = qs.deserialize_str::<Query>("filter[$and][0][col0]=val0&filter[$or][1][col1]=val1");
    assert!(m.is_ok(), "M: {m:?}");

    let m2: Query = qs
      .deserialize_str("filter[col0]=val0&filter[col1]=val1")
      .unwrap();
    assert_eq!(
      m2.filter.unwrap(),
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
