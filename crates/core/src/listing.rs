use std::borrow::Cow;
use thiserror::Error;
use trailbase_qs::ValueOrComposite;
use trailbase_schema::metadata::ColumnMetadata;

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
  column_metadata: &[ColumnMetadata],
  filter_params: Option<ValueOrComposite>,
) -> Result<WhereClause, WhereClauseError> {
  let Some(filter_params) = filter_params else {
    return Ok(WhereClause {
      clause: "TRUE".to_string(),
      params: vec![],
    });
  };

  // Param validation first.
  // NOTE: This is separate step is important, because the value mapping below
  // is **not** applied to all parameters unlike the visitor here.
  filter_params.visit_values(|column_op_value| -> Result<(), WhereClauseError> {
    let column_name = &column_op_value.column;
    if column_name.starts_with("_") {
      return Err(WhereClauseError::UnrecognizedParam(format!(
        "Invalid parameter: {column_name}"
      )));
    }

    return Ok(());
  })?;

  let (sql, params) = filter_params.into_sql(Some(table_name), |column_op_value| {
    let Some(meta) = column_metadata
      .iter()
      .find(|meta| meta.column.name == column_op_value.column)
    else {
      return Err(WhereClauseError::UnrecognizedParam(format!(
        "Filter on unknown column: {}",
        column_op_value.column
      )));
    };

    return crate::records::filter::qs_value_to_sql_with_constraints(
      &meta.column,
      column_op_value.value,
    )
    .map_err(|err| WhereClauseError::UnrecognizedParam(err.to_string()));
  })?;

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
