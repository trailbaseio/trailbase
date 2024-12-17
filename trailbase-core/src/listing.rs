use lazy_static::lazy_static;
use log::*;
use std::borrow::Cow;
use std::collections::HashMap;
use thiserror::Error;

use crate::records::json_to_sql::json_string_to_value;
use crate::table_metadata::TableOrViewMetadata;
use crate::util::b64_to_id;

#[derive(Debug, Error)]
pub enum WhereClauseError {
  #[error("Parse error: {0}")]
  Parse(String),
  #[error("Base64 decoding error: {0}")]
  Base64Decode(#[from] base64::DecodeError),
  #[error("Not implemented error: {0}")]
  NotImplemented(String),
  #[error("Unrecognized param error: {0}")]
  UnrecognizedParam(String),
}

// Syntax: ?key[gte]=value&key[lte]=value
#[derive(Default, Debug, PartialEq)]
pub struct QueryParam {
  pub value: String,
  /// Qualifier or operation such as "greater-than";
  pub qualifier: Option<Qualifier>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Qualifier {
  Not,
  Equal,
  NotEqual,
  GreaterThanEqual,
  GreaterThan,
  LessThanEqual,
  LessThan,
  Like,
  Regexp,
}

impl Qualifier {
  fn from(qualifier: Option<&str>) -> Option<Self> {
    return match qualifier {
      Some("gte") => Some(Self::GreaterThanEqual),
      Some("gt") => Some(Self::GreaterThan),
      Some("lte") => Some(Self::LessThanEqual),
      Some("lt") => Some(Self::LessThan),
      Some("not") => Some(Self::Not),
      Some("ne") => Some(Self::NotEqual),
      Some("like") => Some(Self::Like),
      Some("re") => Some(Self::Regexp),
      None => Some(Self::Equal),
      _ => None,
    };
  }

  fn to_sql(self) -> &'static str {
    return match self {
      Self::GreaterThanEqual => ">=",
      Self::GreaterThan => ">",
      Self::LessThanEqual => "<=",
      Self::LessThan => "<",
      Self::Not => "<>",
      Self::NotEqual => "<>",
      Self::Like => "LIKE",
      Self::Regexp => "REGEXP",
      Self::Equal => "=",
    };
  }
}

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Order {
  Ascending,
  Descending,
}

#[derive(Default, Debug)]
pub struct QueryParseResult {
  // Pagination parameters.
  pub limit: Option<usize>,
  pub cursor: Option<[u8; 16]>,
  pub offset: Option<usize>,

  // Ordering. It's a vector for &order=-col0,+col1,col2
  pub order: Option<Vec<(String, Order)>>,

  // Map from filter params to filter value. It's a vector in cases like
  // "col0[gte]=2&col0[lte]=10".
  pub params: HashMap<String, Vec<QueryParam>>,
}

pub fn limit_or_default(limit: Option<usize>) -> usize {
  const DEFAULT_LIMIT: usize = 50;
  const MAX_LIMIT: usize = 256;

  return std::cmp::min(limit.unwrap_or(DEFAULT_LIMIT), MAX_LIMIT);
}

/// Parses out list-related query params including pagination (limit, cursort), order, and filters.
///
/// An example query may look like:
///  ?cursor=[0:16]&limit=50&order=price,-date&price[lte]=100&date[gte]=<timestamp>.
pub fn parse_query(query: Option<String>) -> Option<QueryParseResult> {
  let q = query?;
  if q.is_empty() {
    return None;
  }

  let mut result: QueryParseResult = Default::default();
  for (key, value) in form_urlencoded::parse(q.as_bytes()) {
    match key.as_ref() {
      "limit" => result.limit = value.parse::<usize>().ok(),
      "cursor" => result.cursor = b64_to_id(value.as_ref()).ok(),
      "offset" => result.offset = value.parse::<usize>().ok(),
      "order" => {
        let order: Vec<(String, Order)> = value
          .split(",")
          .map(|v| {
            return match v {
              x if x.starts_with("-") => (v[1..].to_string(), Order::Descending),
              x if x.starts_with("+") => (v[1..].to_string(), Order::Ascending),
              x => (x.to_string(), Order::Ascending),
            };
          })
          .collect();

        result.order = Some(order);
      }
      key => {
        // Key didn't match any of the predefined list operations (limit, cursor, order), we thus
        // assume it's a column filter. We try to split any qualifier/operation, e.g.
        // column[op]=value.
        let Some((k, maybe_op)) = split_key_into_col_and_op(key) else {
          #[cfg(debug_assertions)]
          debug!("skipping query param: {key}={value}");

          continue;
        };

        if !k
          .chars()
          .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
          #[cfg(debug_assertions)]
          debug!("skipping non-trivial query param: {key}={value}");

          continue;
        }

        if value.is_empty() {
          continue;
        }

        let query_param = QueryParam {
          value: value.to_string(),
          qualifier: Qualifier::from(maybe_op),
        };

        if let Some(v) = result.params.get_mut(k) {
          v.push(query_param)
        } else {
          result.params.insert(k.to_string(), vec![query_param]);
        }
      }
    }
  }

  return Some(result);
}

#[derive(Debug, Clone)]
pub struct WhereClause {
  pub clause: String,
  pub params: Vec<(Cow<'static, str>, trailbase_sqlite::Value)>,
}

pub fn build_filter_where_clause(
  table_metadata: &dyn TableOrViewMetadata,
  filter_params: Option<HashMap<String, Vec<QueryParam>>>,
) -> Result<WhereClause, WhereClauseError> {
  let mut where_clauses = Vec::<String>::with_capacity(16);
  let mut params = Vec::<(Cow<'static, str>, trailbase_sqlite::Value)>::with_capacity(16);

  if let Some(filter_params) = filter_params {
    for (column_name, query_params) in filter_params {
      if column_name.starts_with("_") {
        return Err(WhereClauseError::UnrecognizedParam(format!(
          "Invalid parameter: {column_name}"
        )));
      }

      // IMPORTANT: We only include parameters with known columns to avoid building an invalid
      // query early and forbid injections.
      let Some((col, _col_meta)) = table_metadata.column_by_name(&column_name) else {
        return Err(WhereClauseError::UnrecognizedParam(format!(
          "Unrecognized parameter: {column_name}"
        )));
      };

      for query_param in query_params {
        let Some(op) = query_param.qualifier.map(|q| q.to_sql()) else {
          info!("No op for: {column_name}={query_param:?}");
          continue;
        };

        match json_string_to_value(col.data_type, query_param.value) {
          Ok(value) => {
            where_clauses.push(format!("{column_name} {op} :{column_name}"));
            params.push((format!(":{column_name}").into(), value));
          }
          Err(err) => debug!("Parameter conversion for {column_name} failed: {err}"),
        };
      }
    }
  }

  let clause = match where_clauses.len() {
    0 => "TRUE".to_string(),
    _ => where_clauses.join(" AND "),
  };

  return Ok(WhereClause { clause, params });
}

fn split_key_into_col_and_op(key: &str) -> Option<(&str, Option<&str>)> {
  let Some(captures) = QUALIFIER_REGEX.captures(key) else {
    // Regex didn't match, i.e. key has invalid format.
    return None;
  };

  let Some(k) = captures.name("key") else {
    // No "key" component, i.e. key has invalid format.
    return None;
  };

  return Some((k.as_str(), captures.name("qualifier").map(|c| c.as_str())));
}

lazy_static! {
  /// Regex that splits the key part of "column[op]=value", i.e. column & op.
  static ref QUALIFIER_REGEX: regex::Regex =
    regex::Regex::new(r"^(?<key>\w*)(?:\[(?<qualifier>\w+)\])?$").unwrap();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::util::id_to_b64;

  #[test]
  fn test_op_splitting_regex() {
    assert_eq!(split_key_into_col_and_op("o82@!&#"), None);
    assert_eq!(split_key_into_col_and_op("a b"), None);

    // Check valid column names
    assert_eq!(split_key_into_col_and_op("foo"), Some(("foo", None)));
    assert_eq!(split_key_into_col_and_op("_foo"), Some(("_foo", None)));

    // Check with ops
    assert_eq!(
      split_key_into_col_and_op("_foo[gte]"),
      Some(("_foo", Some("gte")))
    );
    assert_eq!(split_key_into_col_and_op("_foo[$!]"), None);
  }

  #[test]
  fn test_query_parsing() {
    assert!(parse_query(None).is_none());
    assert!(parse_query(Some("".to_string())).is_none());

    {
      let cursor: [u8; 16] = [0; 16];
      // Note that "+" is encoded as %2b, otherwise it's interpreted as a space. That's barely an
      // inconvenience since + is implied and "-" is fine, so there's no real reason to supply "+"
      // explicitly.
      let query = Some(format!(
        "limit=10&cursor={cursor}&order=%2bcol0,-col1,col2",
        cursor = id_to_b64(&cursor)
      ));
      let result = parse_query(query).unwrap();

      assert_eq!(result.limit, Some(10));
      assert_eq!(result.cursor, Some(cursor));
      assert_eq!(
        result.order.unwrap(),
        vec![
          ("col0".to_string(), Order::Ascending),
          ("col1".to_string(), Order::Descending),
          ("col2".to_string(), Order::Ascending),
        ]
      );
    }

    {
      let query = Some("foo,bar&foo_bar&baz=23&bar[like]=foo".to_string());
      let result = parse_query(query).unwrap();

      // foo,bar is an invalid key.
      assert_eq!(result.params.get("foo,bar"), None);
      assert_eq!(result.params.get("foo_bar"), None);
      assert_eq!(
        result.params.get("baz").unwrap(),
        &vec![QueryParam {
          value: "23".to_string(),
          qualifier: Some(Qualifier::Equal),
        }]
      );
      assert_eq!(
        result.params.get("bar").unwrap(),
        &vec![QueryParam {
          value: "foo".to_string(),
          qualifier: Some(Qualifier::Like),
        }]
      );
    }

    {
      // Check whitespaces
      let query = Some("foo=a+b&bar=a%20b".to_string());
      let result = parse_query(query).unwrap();

      assert_eq!(
        result.params.get("foo").unwrap(),
        &vec![QueryParam {
          value: "a b".to_string(),
          qualifier: Some(Qualifier::Equal),
        }]
      );
      assert_eq!(
        result.params.get("bar").unwrap(),
        &vec![QueryParam {
          value: "a b".to_string(),
          qualifier: Some(Qualifier::Equal),
        }]
      );
    }

    {
      let query = Some("col_0[gte]=10&col_0[lte]=100".to_string());
      let result = parse_query(query).unwrap();

      assert_eq!(
        result.params.get("col_0"),
        Some(vec![
          QueryParam {
            value: "10".to_string(),
            qualifier: Some(Qualifier::GreaterThanEqual),
          },
          QueryParam {
            value: "100".to_string(),
            qualifier: Some(Qualifier::LessThanEqual),
          },
        ])
        .as_ref(),
        "{:?}",
        result.params
      );
    }

    {
      // Test both encodings: "+" and %20 for " ".
      let value = "with+white%20spaces";
      let query = Some(format!("text={value}"));
      let result = parse_query(query).unwrap();

      assert_eq!(
        result.params.get("text"),
        Some(vec![QueryParam {
          value: "with white spaces".to_string(),
          qualifier: Some(Qualifier::Equal),
        },])
        .as_ref(),
        "{:?}",
        result.params
      );
    }
  }
}
