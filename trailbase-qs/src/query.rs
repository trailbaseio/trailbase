use base64::prelude::*;
use serde::Deserialize;

use crate::filter::ValueOrComposite;

/// TrailBase supports cursors in a few formats:
///  * Integers
///  * Text-encoded UUIDs ([u8; 16])
///  * Url-safe base64 encoded blobs including UUIDs.
///
/// In practice, we should just support integers and generically blobs. In the future way may want
/// to use encrypted cursors, which would also just be arbitrary url-safe base64 encoded bytes.
#[derive(Clone, Debug, PartialEq)]
pub enum Cursor {
  Blob(Vec<u8>),
  Integer(i64),
}

impl<'de> serde::de::Deserialize<'de> for Cursor {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::de::Deserializer<'de>,
  {
    use serde::de::Error;
    use serde_value::Value;

    let value = Value::deserialize(deserializer)?;
    let Value::String(str) = value else {
      return Err(Error::invalid_type(
        crate::util::unexpected(&value),
        &"comma separated column names",
      ));
    };

    if let Ok(uuid) = uuid::Uuid::parse_str(&str) {
      return Ok(Cursor::Blob(uuid.into()));
    }

    if let Ok(base64) = BASE64_URL_SAFE.decode(&str) {
      return Ok(Cursor::Blob(base64));
    }

    if let Ok(integer) = str.parse::<i64>() {
      return Ok(Cursor::Integer(integer));
    }

    return Err(Error::invalid_type(
      crate::util::unexpected(&Value::String(str)),
      &"integer or url-safe base64 encoded bytes",
    ));
  }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OrderPrecedent {
  Ascending,
  Descending,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Order {
  columns: Vec<(String, OrderPrecedent)>,
}

impl<'de> serde::de::Deserialize<'de> for Order {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::de::Deserializer<'de>,
  {
    use serde::de::Error;
    use serde_value::Value;

    let value = Value::deserialize(deserializer)?;
    let Value::String(str) = value else {
      return Err(Error::invalid_type(
        crate::util::unexpected(&value),
        &"comma separated column names",
      ));
    };

    let columns = str
      .split(",")
      .map(|v| {
        let col_order = match v.trim() {
          x if x.starts_with("-") => (v[1..].to_string(), OrderPrecedent::Descending),
          x if x.starts_with("+") => (v[1..].to_string(), OrderPrecedent::Ascending),
          x => (x.to_string(), OrderPrecedent::Ascending),
        };

        if !crate::util::sanitize_column_name(&col_order.0) {
          return Err(Error::custom(format!(
            "invalid column name: {}",
            col_order.0
          )));
        }

        return Ok(col_order);
      })
      .collect::<Result<Vec<_>, _>>()?;

    if columns.len() > 5 {
      return Err(Error::invalid_length(
        5,
        &"more more than 5 order dimension",
      ));
    }

    return Ok(Order { columns });
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expand {
  columns: Vec<String>,
}

impl<'de> serde::de::Deserialize<'de> for Expand {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::de::Deserializer<'de>,
  {
    use serde::de::Error;
    use serde_value::Value;

    let value = Value::deserialize(deserializer)?;
    let Value::String(str) = value else {
      return Err(Error::invalid_type(
        crate::util::unexpected(&value),
        &"comma separated column names",
      ));
    };

    let columns = str
      .split(",")
      .map(|column_name| {
        if !crate::util::sanitize_column_name(column_name) {
          return Err(Error::custom(
            format!("invalid column name: {column_name}",),
          ));
        }

        return Ok(column_name.to_string());
      })
      .collect::<Result<Vec<_>, _>>()?;

    if columns.len() > 5 {
      return Err(Error::invalid_length(
        5,
        &"more more than 5 expand dimension",
      ));
    }

    return Ok(Expand { columns });
  }
}

#[derive(Clone, Default, Debug, PartialEq, Deserialize)]
pub struct Query {
  /// Pagination parameters:
  ///
  /// Max number of elements returned per page.
  pub limit: Option<usize>,
  /// Cursor to page.
  pub cursor: Option<Cursor>,
  /// Offset to page. Cursor is more efficient when available
  pub offset: Option<usize>,

  /// Return total number of rows in the table.
  pub count: Option<bool>,

  /// Which foreign key columns to expand (only when allowed by configuration).
  pub expand: Option<Expand>,

  /// Ordering. It's a vector for &order=-col0,+col1,col2
  pub order: Option<Order>,

  /// Map from filter params to filter value. It's a vector in cases like:
  ///   `col0[gte]=2&col0[lte]=10`.
  pub filter: Option<ValueOrComposite>,
}

#[cfg(test)]
mod tests {
  use super::*;

  use serde_qs::Config;

  use crate::column_rel_value::{ColumnOpValue, CompareOp};
  use crate::filter::Combiner;
  use crate::value::Value;

  #[test]
  fn test_query_basic_parsing() {
    let qs = Config::new(5, true);

    assert_eq!(qs.deserialize_str::<Query>("").unwrap(), Query::default());
  }

  #[test]
  fn test_query_order_parsing() {
    let qs = Config::new(5, true);

    assert_eq!(
      qs.deserialize_str::<Query>("order=").unwrap(),
      Query {
        order: None,
        ..Default::default()
      },
    );

    assert!(qs.deserialize_str::<Query>("order=$").is_err());
    assert!(qs.deserialize_str::<Query>("order=a,b,c,d,e").is_ok());
    assert!(qs.deserialize_str::<Query>("order=a,b,c,d,e,f").is_err());

    assert_eq!(
      qs.deserialize_str::<Query>("order=a,-b,+c").unwrap(),
      Query {
        order: Some(Order {
          columns: vec![
            ("a".to_string(), OrderPrecedent::Ascending),
            ("b".to_string(), OrderPrecedent::Descending),
            ("c".to_string(), OrderPrecedent::Ascending),
          ]
        }),
        ..Default::default()
      }
    );
  }

  #[test]
  fn test_query_expand_parsing() {
    let qs = Config::new(5, true);

    assert_eq!(
      qs.deserialize_str::<Query>("expand=").unwrap(),
      Query {
        expand: None,
        ..Default::default()
      },
    );

    assert!(qs.deserialize_str::<Query>("expand=$").is_err());
    assert!(qs.deserialize_str::<Query>("expand=a,b,c,d,e").is_ok());
    assert!(qs.deserialize_str::<Query>("expand=a,b,c,d,e,f").is_err());
  }

  #[test]
  fn test_query_filter_parsing() {
    let qs = Config::new(5, true);

    assert_eq!(
      qs.deserialize_str::<Query>("filter=").unwrap(),
      Query::default()
    );

    let q0: Query = qs
      .deserialize_str("filter[col0][gt]=0&filter[col1]=val1")
      .unwrap();
    assert_eq!(
      q0.filter.unwrap(),
      ValueOrComposite::Composite(
        Combiner::And,
        vec![
          ValueOrComposite::Value(ColumnOpValue {
            column: "col0".to_string(),
            op: CompareOp::GreaterThan,
            value: Value::Integer(0),
          }),
          ValueOrComposite::Value(ColumnOpValue {
            column: "col1".to_string(),
            op: CompareOp::Equal,
            value: Value::String("val1".to_string()),
          }),
        ]
      )
    );

    // Implicit and with nested or and out of order.
    let q1: Query = qs
      .deserialize_str("filter[$or][1][col0][ne]=val0&filter[col1]=1&filter[$or][0][col2]=val2")
      .unwrap();
    assert_eq!(
      q1.filter.as_ref().unwrap(),
      &ValueOrComposite::Composite(
        Combiner::And,
        vec![
          ValueOrComposite::Composite(
            Combiner::Or,
            vec![
              ValueOrComposite::Value(ColumnOpValue {
                column: "col2".to_string(),
                op: CompareOp::Equal,
                value: Value::String("val2".to_string()),
              }),
              ValueOrComposite::Value(ColumnOpValue {
                column: "col0".to_string(),
                op: CompareOp::NotEqual,
                value: Value::String("val0".to_string()),
              }),
            ]
          ),
          ValueOrComposite::Value(ColumnOpValue {
            column: "col1".to_string(),
            op: CompareOp::Equal,
            value: Value::Integer(1),
          }),
        ]
      )
    );
    assert_eq!(
      q1.filter.unwrap().to_sql(),
      "((col2 = 'val2' OR col0 <> 'val0') AND col1 = 1)"
    );
  }

  #[test]
  fn test_query_cursor_parsing() {
    let qs = Config::new(5, true);

    assert_eq!(
      qs.deserialize_str::<Query>("cursor=").unwrap(),
      Query::default()
    );

    assert_eq!(
      qs.deserialize_str::<Query>("cursor=-5").unwrap(),
      Query {
        cursor: Some(Cursor::Integer(-5)),
        ..Default::default()
      }
    );

    let uuid = uuid::Uuid::now_v7();
    assert_eq!(
      qs.deserialize_str::<Query>(&format!("cursor={}", uuid.to_string()))
        .unwrap(),
      Query {
        cursor: Some(Cursor::Blob(uuid.as_bytes().into())),
        ..Default::default()
      }
    );

    let blob = BASE64_URL_SAFE.encode(uuid.as_bytes());
    assert_eq!(
      qs.deserialize_str::<Query>(&format!("cursor={blob}"))
        .unwrap(),
      Query {
        cursor: Some(Cursor::Blob(uuid.as_bytes().into())),
        ..Default::default()
      }
    );
  }
}
