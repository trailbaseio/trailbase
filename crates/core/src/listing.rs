use base64::prelude::*;
use log::*;
use std::borrow::Cow;
use thiserror::Error;
use trailbase_qs::{Cursor as QsCursor, Value as QsValue, ValueOrComposite};
use trailbase_schema::sqlite::Column;

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

pub(crate) fn cursor_to_value(cursor: QsCursor) -> trailbase_sqlite::Value {
  return match cursor {
    QsCursor::Integer(i) => trailbase_sqlite::Value::Integer(i),
    QsCursor::Blob(b) => trailbase_sqlite::Value::Blob(b),
  };
}

#[derive(Debug, Clone)]
pub struct WhereClause {
  pub clause: String,
  pub params: Vec<(Cow<'static, str>, trailbase_sqlite::Value)>,
}

pub(crate) fn build_filter_where_clause(
  table_name: &str,
  columns: &[Column],
  filter_params: Option<ValueOrComposite>,
) -> Result<WhereClause, WhereClauseError> {
  let Some(filter_params) = filter_params else {
    return Ok(WhereClause {
      clause: "TRUE".to_string(),
      params: vec![],
    });
  };

  let validator = |column_name: &str| -> Result<(), WhereClauseError> {
    if column_name.starts_with("_") {
      return Err(WhereClauseError::UnrecognizedParam(format!(
        "Invalid parameter: {column_name}"
      )));
    }

    // IMPORTANT: We only include parameters with known columns to avoid building an invalid
    // query early and forbid injections.
    if !columns.iter().any(|c| c.name == column_name) {
      return Err(WhereClauseError::UnrecognizedParam(format!(
        "Unrecognized parameter: {column_name}"
      )));
    };

    return Ok(());
  };

  let (sql, params) = filter_params.into_sql(Some(table_name), &validator)?;

  use trailbase_sqlite::Value;
  type Param = (Cow<'static, str>, Value);
  let sql_params: Vec<Param> = params
    .into_iter()
    .map(|(name, value)| {
      return (
        Cow::Owned(name),
        match value {
          QsValue::String(s) => {
            if let Ok(b) = BASE64_URL_SAFE.decode(&s) {
              Value::Blob(b)
            } else {
              Value::Text(s)
            }
          }
          QsValue::Integer(i) => Value::Integer(i),
          QsValue::Double(d) => Value::Real(d),
          QsValue::Bool(b) => Value::Integer(if b { 1 } else { 0 }),
        },
      );
    })
    .collect();

  return Ok(WhereClause {
    clause: sql,
    params: sql_params,
  });
}

pub fn limit_or_default(
  limit: Option<usize>,
  hard_limit: Option<usize>,
) -> Result<usize, &'static str> {
  const DEFAULT_LIMIT: usize = 50;
  const DEFAULT_HARD_LIMIT: usize = 1024;

  if let Some(limit) = limit {
    if limit > hard_limit.unwrap_or(DEFAULT_HARD_LIMIT) {
      return Err("limit exceeds max limit of 1024");
    }
    return Ok(limit);
  }
  return Ok(limit.unwrap_or(DEFAULT_LIMIT));
}
