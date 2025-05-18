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
    prefix: Option<&str>,
    validator: &dyn Fn(&str) -> Result<(), E>,
  ) -> Result<(String, Vec<(String, Value)>), E> {
    let mut index: usize = 0;
    return self.into_sql_impl(prefix, validator, &mut index);
  }

  fn into_sql_impl<E>(
    self,
    prefix: Option<&str>,
    validator: &dyn Fn(&str) -> Result<(), E>,
    index: &mut usize,
  ) -> Result<(String, Vec<(String, Value)>), E> {
    match self {
      Self::Value(v) => {
        validator(&v.column)?;

        let param = param_name(*index);
        *index += 1;

        return Ok((
          match prefix {
            Some(p) => format!(r#"{p}."{c}" {o} {param}"#, c = v.column, o = v.op.to_sql()),
            None => format!(r#""{c}" {o} {param}"#, c = v.column, o = v.op.to_sql()),
          },
          vec![(param, v.value)],
        ));
      }
      Self::Composite(combiner, vec) => {
        let mut fragments = Vec::<String>::with_capacity(vec.len());
        let mut params = Vec::<(String, Value)>::with_capacity(vec.len());

        for value_or_composite in vec {
          let (f, p) = value_or_composite.into_sql_impl::<E>(prefix, validator, index)?;
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

  if m.is_empty() {
    // We could also error here, but this allows empty query-string.
    return Ok(ValueOrComposite::Composite(Combiner::And, vec![]));
  } else if m.len() > 1 {
    // Multiple entries on the same same level => Implicit AND composite
    let vec = m
      .into_iter()
      .map(|(k, v)| {
        return match k {
          Value::String(key) => match key.as_str() {
            "$and" | "$or" => serde_value_to_value_or_composite::<D>(
              Value::Map(BTreeMap::from([(Value::String(key), v)])),
              depth + 1,
            ),
            _ => Ok(ValueOrComposite::Value(
              serde_value_to_single_column_rel_value::<D>(key, v)?,
            )),
          },
          _ => Err(Error::invalid_type(
            crate::util::unexpected(&k),
            &"string key",
          )),
        };
      })
      .collect::<Result<Vec<_>, _>>()?;

    return Ok(ValueOrComposite::Composite(Combiner::And, vec));
  }

  let combine = |combiner: Combiner, values: Value| -> Result<ValueOrComposite, D::Error> {
    match values {
      Value::Seq(vec) => {
        if vec.len() < 2 {
          return Err(serde::de::Error::invalid_length(
            vec.len(),
            &"Sequence with 2 or more elements",
          ));
        }

        return Ok(ValueOrComposite::Composite(
          combiner,
          vec
            .into_iter()
            .map(|v| serde_value_to_value_or_composite::<D>(v, depth + 1))
            .collect::<Result<Vec<_>, _>>()?,
        ));
      }
      v => Err(Error::invalid_type(
        crate::util::unexpected(&v),
        &"Sequence",
      )),
    }
  };

  match m.pop_first().expect("len == 1") {
    (Value::String(str), v) => match str.as_str() {
      "$and" => combine(Combiner::And, v),
      "$or" => combine(Combiner::Or, v),
      _ => Ok(ValueOrComposite::Value(
        serde_value_to_single_column_rel_value::<D>(str, v)?,
      )),
    },
    (k, _) => Err(Error::invalid_type(crate::util::unexpected(&k), &"String")),
  }
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

    let m0: Result<Query, _> = qs.deserialize_str("filter[$and][0][col0]=val0");
    assert!(m0.is_err(), "{m0:?}");

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

    assert!(
      qs.deserialize_str::<Query>("filter[$and][0][col0]=val0&filter[$or][1][col1]=val1",)
        .is_err()
    );

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

    // Too few elements
    let m4: Result<Query, _> = qs.deserialize_str("filter[$and][0][col0]=val0");
    assert!(m4.is_err(), "{m4:?}");

    // Too few elements
    let m3: Result<Query, _> =
      qs.deserialize_str("filter[col0]=val0&filter[$and][0][col0]=val0&filter[col1]=val1");
    assert!(m3.is_err(), "{m3:?}");
  }
}
