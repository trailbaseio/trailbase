use log::*;
use std::borrow::Cow;
use thiserror::Error;
use trailbase_qs::ValueOrComposite;
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

  let convert = |column_name: &str,
                 value: trailbase_qs::Value|
   -> Result<trailbase_sqlite::Value, WhereClauseError> {
    if column_name.starts_with("_") {
      return Err(WhereClauseError::UnrecognizedParam(format!(
        "Invalid parameter: {column_name}"
      )));
    }

    let Some(column) = columns.iter().find(|c| c.name == column_name) else {
      return Err(WhereClauseError::UnrecognizedParam(format!(
        "Unrecognized parameter: {column_name}"
      )));
    };

    // TODO: Improve hacky error handling.
    return crate::records::filter::qs_value_to_sql_with_constraints(column, value)
      .map_err(|err| WhereClauseError::UnrecognizedParam(err.to_string()));
  };

  let (sql, params) = filter_params.into_sql(Some(table_name), &convert)?;

  return Ok(WhereClause {
    clause: sql,
    params: params
      .into_iter()
      .map(|(name, v)| (Cow::Owned(name), v))
      .collect(),
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
