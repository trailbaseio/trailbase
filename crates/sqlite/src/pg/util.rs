use postgres::fallible_iterator::FallibleIterator;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use crate::error::Error;
use crate::params::Params;
use crate::rows::{Column, Row, Rows, ValueType};
use crate::statement::Statement;
use crate::to_sql::ToSqlProxy;
use crate::value::Value;

#[derive(Debug)]
pub(crate) struct PgStatement<'a> {
  #[allow(unused)]
  sql: &'a str,

  // TODO: Could we use ToSqlProxy here to reduce copies?
  params: Vec<(usize, Value)>,
  placeholders: HashMap<String, usize>,
}

impl<'a> PgStatement<'a> {
  pub fn new(sql: &'a str) -> Result<Self, Error> {
    static NAMED_RE: LazyLock<Regex> =
      LazyLock::new(|| Regex::new(r"(?<named>:[[:alpha:]][[:alnum:]]*)").expect("startup"));

    let mut placeholders: HashMap<String, usize> = Default::default();
    for (idx, cap) in NAMED_RE.captures_iter(sql).enumerate() {
      let named_params = &cap["named"];
      placeholders.insert(named_params.to_string(), idx + 1);
    }

    return Ok(Self {
      sql,
      params: vec![],
      placeholders,
    });
  }

  pub fn bind(mut self, params: impl Params) -> Result<(String, Vec<Value>), Error> {
    params.bind(&mut self)?;

    let Self {
      sql,
      placeholders,
      mut params,
    } = self;

    // TODO: Do we need further validation, e.g. that indexes are consecutive, that they're
    // matching the SQL...?
    let bound_params = {
      params.sort_by(|a, b| {
        return a.0.cmp(&b.0);
      });
      params.into_iter().map(|p| p.1).collect()
    };

    // Also support "?1" placeholders like sqlite (PG only supports $1).
    static RE: LazyLock<Regex> =
      LazyLock::new(|| Regex::new(r"[?](?<index>\d+)").expect("startup"));

    let mut sql = RE.replace_all(sql, "$$$index").to_string();

    // TODO: We should probably do this along the initial parse when we find the placeholders.
    for (name, idx) in placeholders {
      sql = sql.replace(&name, &format!("${idx}"));
    }

    return Ok((sql, bound_params));
  }
}

impl<'a> Statement for PgStatement<'a> {
  fn bind_parameter(&mut self, one_based_index: usize, param: ToSqlProxy<'_>) -> Result<(), Error> {
    self.params.push((one_based_index, param.try_into()?));
    return Ok(());
  }

  /// Will return Err if `name` is invalid. Will return Ok(None) if the name
  /// is valid but not a bound parameter of this statement.
  fn parameter_index(&self, name: &str) -> Result<Option<usize>, Error> {
    if &name[0..1] != ":" || name[1..].chars().any(|c| !c.is_ascii_alphanumeric()) {
      return Err(Error::Other(format!("invalid param name: {name}").into()));
    }
    return Ok(self.placeholders.get(name).cloned());
  }
}

#[inline]
pub(crate) fn map_first<T>(
  mut rows: postgres::RowIter<'_>,
  f: impl (FnOnce(postgres::Row) -> Result<T, Error>) + Send + 'static,
) -> Result<Option<T>, Error>
where
  T: Send + 'static,
{
  if let Some(row) = rows.next()? {
    return Ok(Some(f(row)?));
  }
  return Ok(None);
}

pub fn from_rows(mut row_iter: postgres::RowIter) -> Result<Rows, Error> {
  let Some(first_row) = row_iter.next()? else {
    return Ok(Rows::default());
  };

  let columns: Arc<Vec<Column>> = Arc::new(columns(&first_row));

  let mut result = vec![self::from_row(&first_row, columns.clone())?];
  while let Some(row) = row_iter.next()? {
    result.push(self::from_row(&row, columns.clone())?);
  }

  return Ok(Rows(result, columns));
}

pub(crate) fn from_row(row: &postgres::Row, cols: Arc<Vec<Column>>) -> Result<Row, Error> {
  #[cfg(debug_assertions)]
  if let Some(rc) = Some(columns(row))
    && rc.len() != cols.len()
  {
    // Apparently this can happen during schema manipulations, e.g. when deleting a column
    // :shrug:. We normalize everything to the same rows schema rather than dealing with
    // jagged tables.
    log::warn!("Rows/row column mismatch: {cols:?} vs {rc:?}");
  }

  // We have to access by index here, since names can be duplicate.
  let values = (0..cols.len())
    .map(|idx| row.try_get::<usize, Value>(idx).unwrap_or(Value::Null))
    .collect();

  return Ok(Row(values, cols));
}

#[inline]
pub(crate) fn columns(row: &postgres::Row) -> Vec<Column> {
  return row
    .columns()
    .iter()
    .map(|c| Column {
      name: c.name().to_string(),
      decl_type: match c.type_().name() {
        "int8" | "int4" => Some(ValueType::Integer),
        "float8" | "float4" => Some(ValueType::Real),
        "text" | "varchar" => Some(ValueType::Text),
        "bytea" => Some(ValueType::Blob),
        _ => None,
      },
    })
    .collect();
}

#[cfg(test)]
mod tests {
  use crate::named_params;

  use super::*;

  #[test]
  fn pg_statement_test() {
    let (sql, params) = PgStatement::new("INSERT INTO 'table' (col) VALUES (?1), (?1)")
      .unwrap()
      .bind(("foo",))
      .unwrap();

    assert_eq!("INSERT INTO 'table' (col) VALUES ($1), ($1)", sql);
    assert_eq!(Value::Text("foo".to_string()), *params.first().unwrap());

    let (sql, params) = PgStatement::new("INSERT INTO 'table' (col) VALUES (:p0), (:p1)")
      .unwrap()
      .bind(named_params! {":p0": "p0", ":p1": "p1"})
      .unwrap();

    assert_eq!("INSERT INTO 'table' (col) VALUES ($1), ($2)", sql);
    assert_eq!(
      vec![Value::Text("p0".to_string()), Value::Text("p1".to_string())],
      params,
    );
  }
}
