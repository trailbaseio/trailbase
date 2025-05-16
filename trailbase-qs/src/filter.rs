/// Custom deserializers for urlencoded filters.
///
/// Supported examples:
///
/// filters[column]=value  <-- might be nice otherwise below.
/// filter[column]=value
/// filters[column][eq]=value
/// filters[and][0][column0][eq]=value0&filters[and][1][column1][eq]=value1
/// filters[and][0][or][0][column0]=value0&[and][0][or][1][column1]=value1
use itertools::Itertools;
use std::collections::BTreeMap;

use crate::column_rel_value::{ColumnOpValue, serde_value_to_single_column_rel_value};

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
  #[allow(unused)]
  pub fn to_sql(&self) -> String {
    let mut fragments: Vec<String> = vec![];
    match self {
      Self::Value(v) => fragments.push(v.to_sql()),
      Self::Composite(combiner, vec) => {
        let f = vec.iter().map(|v| v.to_sql()).join(match combiner {
          Combiner::And => " AND ",
          Combiner::Or => " OR ",
        });
        fragments.push(format!("({f})"));
      }
    };
    return fragments.join(" ");
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
      &"map[col]=val or map[col][op]=val",
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

  // We iterate only to take ownership.
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

#[cfg(test)]
mod tests {
  use super::*;

  use serde::Deserialize;
  use serde_qs::Config;

  use crate::column_rel_value::{ColumnOpValue, CompareOp};
  use crate::value::Value;

  #[derive(Clone, Debug, Default, Deserialize)]
  struct Query {
    composite_filter: Option<ValueOrComposite>,
  }

  #[test]
  fn test_filter_parsing() {
    let qs = Config::new(5, true);

    let m_empty: Query = qs.deserialize_str("").unwrap();
    assert_eq!(m_empty.composite_filter, None);

    let m0: Result<Query, _> = qs.deserialize_str("composite_filter[$and][0][col0]=val0");
    assert!(m0.is_err(), "{m0:?}");

    let m1: Query = qs
      .deserialize_str("composite_filter[$and][0][col0]=val0&composite_filter[$and][1][col1]=val1")
      .unwrap();
    assert_eq!(
      m1.composite_filter.unwrap(),
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
      qs.deserialize_str::<Query>(
        "composite_filter[$and][0][col0]=val0&composite_filter[$or][1][col1]=val1",
      )
      .is_err()
    );

    let m2: Query = qs
      .deserialize_str("composite_filter[col0]=val0&composite_filter[col1]=val1")
      .unwrap();
    assert_eq!(
      m2.composite_filter.unwrap(),
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
    let m4: Result<Query, _> = qs.deserialize_str("composite_filter[$and][0][col0]=val0");
    assert!(m4.is_err(), "{m4:?}");

    // Too few elements
    let m3: Result<Query, _> =
            qs.deserialize_str("composite_filter[col0]=val0&composite_filter[$and][0][col0]=val0&composite_filter[col1]=val1");
    assert!(m3.is_err(), "{m3:?}");
  }
}
