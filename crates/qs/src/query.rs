use base64::prelude::*;
use serde::Deserialize;

use crate::filter::ValueOrComposite;
use crate::util::deserialize_bool;

pub type Error = serde_qs::Error;

#[derive(Clone, Debug, PartialEq)]
pub enum CursorType {
  Blob,
  Integer,
}

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

impl Cursor {
  pub fn parse(s: &str, cursor_type: CursorType) -> Result<Self, Error> {
    return match cursor_type {
      CursorType::Integer => {
        let i = s.parse::<i64>().map_err(Error::ParseInt)?;
        Ok(Self::Integer(i))
      }
      CursorType::Blob => {
        if let Ok(uuid) = uuid::Uuid::parse_str(s) {
          return Ok(Cursor::Blob(uuid.into()));
        }

        if let Ok(base64) = BASE64_URL_SAFE.decode(s) {
          return Ok(Cursor::Blob(base64));
        }

        Err(Error::Custom(format!("Failed to parse: {s}")))
      }
    };
  }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OrderPrecedent {
  Ascending,
  Descending,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Order {
  pub columns: Vec<(String, OrderPrecedent)>,
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
        &"comma separated column names to order by",
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
            "invalid column name for order: {}",
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
  pub columns: Vec<String>,
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
        &"comma separated foreign-key column names to expand",
      ));
    };

    let columns = str
      .split(",")
      .map(|column_name| {
        if !crate::util::sanitize_column_name(column_name) {
          return Err(Error::custom(format!(
            "invalid column name for expand: {column_name}",
          )));
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
  pub cursor: Option<String>,
  /// Offset to page. Cursor is more efficient when available
  pub offset: Option<usize>,

  /// Return total number of rows in the table.
  #[serde(default, deserialize_with = "deserialize_bool")]
  pub count: Option<bool>,

  /// Which foreign key columns to expand (only when allowed by configuration).
  pub expand: Option<Expand>,

  /// Ordering. It's a vector for &order=-col0,+col1,col2
  pub order: Option<Order>,

  /// Map from filter params to filter value. It's a vector in cases like:
  ///   `col0[$gte]=2&col0[$lte]=10`.
  pub filter: Option<ValueOrComposite>,
}

impl Query {
  pub fn parse(query: &str) -> Result<Query, Error> {
    // NOTE: We rely on non-strict mode to parse `filter[col0]=a&b%filter[col1]=c`.
    let qs = serde_qs::Config::new(9, false);
    return qs.deserialize_bytes::<Query>(query.as_bytes());
  }

  /// Produce a query-string representation of this `Query`.
  pub fn stringify(&self) -> String {
    let mut pairs: Vec<(String, String)> = Vec::new();

    if let Some(limit) = self.limit {
      pairs.push(("limit".to_string(), limit.to_string()));
    }

    if let Some(ref cursor) = self.cursor {
      pairs.push(("cursor".to_string(), crate::filter::encode_val(cursor)));
    }

    if let Some(offset) = self.offset {
      pairs.push(("offset".to_string(), offset.to_string()));
    }

    if let Some(count) = self.count {
      pairs.push((
        "count".to_string(),
        (if count { "true" } else { "false" }).to_string(),
      ));
    }

    if let Some(ref expand) = self.expand {
      let s = expand.columns.join(",");
      pairs.push(("expand".to_string(), crate::filter::encode_val(&s)));
    }

    if let Some(ref order) = self.order {
      let s = order
        .columns
        .iter()
        .map(|(c, p)| match p {
          crate::query::OrderPrecedent::Descending => format!("-{}", c),
          crate::query::OrderPrecedent::Ascending => format!("{}", c),
        })
        .collect::<Vec<_>>()
        .join(",");

      pairs.push(("order".to_string(), crate::filter::encode_val(&s)));
    }

    if let Some(ref filter) = self.filter {
      pairs.extend(filter.into_qs_impl("filter"));
    }

    return pairs
      .into_iter()
      .map(|(k, v)| format!("{}={}", crate::filter::encode_key(&k), v))
      .collect::<Vec<_>>()
      .join("&");
  }
}

#[derive(Clone, Default, Debug, PartialEq, Deserialize)]
pub struct FilterQuery {
  /// Map from filter params to filter value. It's a vector in cases like:
  ///   `col0[$gte]=2&col0[$lte]=10`.
  pub filter: Option<ValueOrComposite>,
}

impl FilterQuery {
  pub fn parse(query: &str) -> Result<FilterQuery, Error> {
    // NOTE: We rely on non-strict mode to parse `filter[col0]=a&b%filter[col1]=c`.
    let qs = serde_qs::Config::new(9, false);
    return qs.deserialize_bytes::<FilterQuery>(query.as_bytes());
  }

  /// Produce query string for only the filter part.
  pub fn stringify(&self) -> String {
    if let Some(ref filter) = self.filter {
      return filter.into_qs();
    }
    return "".to_string();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use rusqlite::types::Value as SqlValue;
  use serde_qs::Config;

  use crate::column_rel_value::{ColumnOpValue, CompareOp};
  use crate::filter::Combiner;
  use crate::value::Value;

  #[test]
  fn test_query_basic_parsing() {
    assert_eq!(Query::parse("").unwrap(), Query::default());
    assert_eq!(Query::parse("unknown=foo").unwrap(), Query::default());

    // NOTE: The filter value contains a '&', which will not parse in serde_qs strict-mode. Test
    // explicitly that we properly allow '&'s.
    assert_eq!(
      Query::parse("filter%5Btext_not_null%5D=rust+client+test+0%3A+%3D%3F%261747466199")
        .unwrap()
        .filter
        .unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "text_not_null".to_string(),
        op: CompareOp::Equal,
        value: Value::String("rust client test 0: =?&1747466199".to_string()),
      })
    );

    let expected = ValueOrComposite::Composite(
      Combiner::And,
      vec![
        ValueOrComposite::Composite(
          Combiner::Or,
          vec![
            ValueOrComposite::Value(ColumnOpValue {
              column: "latency".to_string(),
              op: CompareOp::GreaterThan,
              value: Value::Integer(2),
            }),
            ValueOrComposite::Value(ColumnOpValue {
              column: "status".to_string(),
              op: CompareOp::GreaterThanEqual,
              value: Value::Integer(400),
            }),
          ],
        ),
        ValueOrComposite::Value(ColumnOpValue {
          column: "latency".to_string(),
          op: CompareOp::GreaterThan,
          value: Value::Integer(2),
        }),
      ],
    );

    // Make sure depth in the parse config is set large enough to also parse more deeply composed
    // expressions.
    assert_eq!(
      Query::parse("filter[$and][0][$or][0][latency][$gt]=2&filter[$and][0][$or][1][status][$gte]=400&filter[$and][1][latency][$gt]=2")
        .unwrap()
        .filter
        .unwrap(),
        expected
    );

    assert_eq!(
      Query::parse("limit=5&offset=5&count=true").unwrap(),
      Query {
        limit: Some(5),
        offset: Some(5),
        count: Some(true),
        ..Default::default()
      }
    );
    assert_eq!(
      Query::parse("count=FALSE").unwrap(),
      Query {
        count: Some(false),
        ..Default::default()
      }
    );
    assert!(Query::parse("offset=-1").is_err());
  }

  #[test]
  fn test_query_stringify_basic() {
    let q = Query {
      limit: Some(10),
      cursor: Some("-5".to_string()),
      offset: Some(2),
      count: Some(true),
      expand: Some(Expand {
        columns: vec!["a".to_string(), "b".to_string()],
      }),
      order: Some(Order {
        columns: vec![
          ("a".to_string(), OrderPrecedent::Ascending),
          ("b".to_string(), OrderPrecedent::Descending),
        ],
      }),
      filter: None,
    };

    let s = q.stringify();
    // Order of params isn't strictly specified; check presence of important fragments.
    assert!(s.contains("limit=10"));
    assert!(s.contains("cursor=-5"));
    assert!(s.contains("offset=2"));
    assert!(s.contains("count=true"));
    assert!(s.contains("expand=a%2Cb") || s.contains("expand=a,b"));
    assert!(s.contains("order=a%2C-b") || s.contains("order=a,-b"));
  }

  #[test]
  fn test_filter_stringify_simple_and_composite() {
    use crate::column_rel_value::{ColumnOpValue, CompareOp};

    let f1 = ValueOrComposite::Value(ColumnOpValue {
      column: "col0".to_string(),
      op: CompareOp::Equal,
      value: crate::value::Value::String("val0".to_string()),
    });

    assert_eq!(f1.into_qs(), "filter%5Bcol0%5D=val0");

    let f2 = ValueOrComposite::Composite(
      Combiner::And,
      vec![
        f1.clone(),
        ValueOrComposite::Value(ColumnOpValue {
          column: "col1".to_string(),
          op: CompareOp::Equal,
          value: crate::value::Value::String("val1".to_string()),
        }),
      ],
    );

    let s = f2.into_qs();
    // Two key/value pairs; ordering of pairs is deterministic by our implementation
    assert_eq!(
      s,
      "filter%5B%24and%5D%5B0%5D%5Bcol0%5D=val0&filter%5B%24and%5D%5B1%5D%5Bcol1%5D=val1"
    );
  }

  #[test]
  fn test_query_order_parsing() {
    let qs = Config::new(5, false);

    assert_eq!(
      Query::parse("order=").unwrap(),
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
    let qs = Config::new(5, false);

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
    let qs = Config::new(5, false);

    assert_eq!(
      qs.deserialize_str::<Query>("filter=").unwrap(),
      Query::default()
    );

    let q0: Query = qs
      .deserialize_str("filter[col0][$gt]=0&filter[col1]=val1")
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
      .deserialize_str("filter[$or][1][col0][$ne]=val0&filter[col1]=1&filter[$or][0][col2]=val2")
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

    let filter = |_: &str, value: Value| -> Result<SqlValue, String> {
      return Ok(match value {
        Value::String(s) => SqlValue::Text(s),
        Value::Integer(i) => SqlValue::Integer(i),
        Value::Double(d) => SqlValue::Real(d),
      });
    };
    let (sql, params) = q1.filter.clone().unwrap().into_sql(None, &filter).unwrap();
    assert_eq!(
      sql,
      r#"(("col2" = :__p0 OR "col0" <> :__p1) AND "col1" = :__p2)"#
    );
    assert_eq!(
      params,
      vec![
        (":__p0".to_string(), SqlValue::Text("val2".to_string())),
        (":__p1".to_string(), SqlValue::Text("val0".to_string())),
        (":__p2".to_string(), SqlValue::Integer(1)),
      ]
    );
    let (sql, _) = q1.filter.unwrap().into_sql(Some("p"), &filter).unwrap();
    assert_eq!(
      sql,
      r#"((p."col2" = :__p0 OR p."col0" <> :__p1) AND p."col1" = :__p2)"#
    );

    // Test both encodings: '+' and %20 for ' '.
    let q2: Query = qs
      .deserialize_str("filter[col]=with+white%20spaces")
      .unwrap();
    assert_eq!(
      q2.filter.unwrap(),
      ValueOrComposite::Value(ColumnOpValue {
        column: "col".to_string(),
        op: CompareOp::Equal,
        value: Value::String("with white spaces".to_string()),
      }),
    );
  }

  #[test]
  fn test_date_range_filter() {
    // Test that multiple operators on the same column (e.g., date range filters) work correctly
    let result =
      Query::parse("filter[datetime][$gte]=2025-09-25&filter[datetime][$lte]=2025-09-27");

    let query = result.expect("Should parse date range filter");
    let filter = query.filter.expect("Should have filter");

    // Verify it creates an AND composite with two conditions
    match filter {
      ValueOrComposite::Composite(Combiner::And, values) => {
        assert_eq!(values.len(), 2, "Should have two date conditions");

        // Check the conditions are correct
        if let ValueOrComposite::Value(first) = &values[0] {
          assert_eq!(first.column, "datetime");
          assert_eq!(first.op, CompareOp::GreaterThanEqual);
        }

        if let ValueOrComposite::Value(second) = &values[1] {
          assert_eq!(second.column, "datetime");
          assert_eq!(second.op, CompareOp::LessThanEqual);
        }
      }
      _ => panic!("Expected AND composite filter for date range"),
    }
  }

  #[test]
  fn test_query_cursor_parsing() {
    let qs = Config::new(5, false);

    assert_eq!(
      qs.deserialize_str::<Query>("cursor=").unwrap(),
      Query::default()
    );

    assert_eq!(
      qs.deserialize_str::<Query>("cursor=-5").unwrap(),
      Query {
        cursor: Some("-5".to_string()),
        ..Default::default()
      }
    );

    let uuid = uuid::Uuid::now_v7();
    let r = qs
      .deserialize_str::<Query>(&format!("cursor={}", uuid.to_string()))
      .unwrap();
    assert_eq!(
      r,
      Query {
        cursor: Some(uuid.to_string()),
        ..Default::default()
      }
    );
    assert_eq!(
      Cursor::parse(&r.cursor.unwrap(), CursorType::Blob).unwrap(),
      Cursor::Blob(uuid.into())
    );

    let blob = BASE64_URL_SAFE.encode(uuid.as_bytes());
    assert_eq!(
      qs.deserialize_str::<Query>(&format!("cursor={blob}"))
        .unwrap(),
      Query {
        cursor: Some(blob),
        ..Default::default()
      }
    );
  }
}
