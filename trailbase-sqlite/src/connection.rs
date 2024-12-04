pub use rusqlite::types::{ToSqlOutput, Value};
use rusqlite::{types, Statement};
use std::{
  cell::RefCell,
  fmt::{self, Debug},
  str::FromStr,
  sync::Arc,
};

use crate::error::Error;
pub use crate::params::Params;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[macro_export]
macro_rules! params {
    () => {
        [] as [$crate::params::ToSqlType]
    };
    ($($param:expr),+ $(,)?) => {
        [$(Into::<$crate::params::ToSqlType>::into($param)),+]
    };
}

#[macro_export]
macro_rules! named_params {
    () => {
        [] as [(&str, $crate::params::ToSqlType)]
    };
    ($($param_name:literal: $param_val:expr),+ $(,)?) => {
        [$(($param_name as &str, Into::<$crate::params::ToSqlType>::into($param_val))),+]
    };
}

/// The result returned on method calls in this crate.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Copy, Clone)]
pub enum ValueType {
  Integer = 1,
  Real,
  Text,
  Blob,
  Null,
}

impl FromStr for ValueType {
  type Err = ();

  fn from_str(s: &str) -> std::result::Result<ValueType, Self::Err> {
    match s {
      "TEXT" => Ok(ValueType::Text),
      "INTEGER" => Ok(ValueType::Integer),
      "BLOB" => Ok(ValueType::Blob),
      "NULL" => Ok(ValueType::Null),
      "REAL" => Ok(ValueType::Real),
      _ => Err(()),
    }
  }
}

#[allow(unused)]
#[derive(Debug)]
pub struct Column {
  name: String,
  decl_type: Option<ValueType>,
}

#[derive(Debug)]
pub struct Rows(Vec<Row>, Arc<Vec<Column>>);

fn columns(stmt: &Statement<'_>) -> Vec<Column> {
  return stmt
    .columns()
    .into_iter()
    .map(|c| Column {
      name: c.name().to_string(),
      decl_type: c.decl_type().and_then(|s| ValueType::from_str(s).ok()),
    })
    .collect();
}

impl Rows {
  pub fn from_rows(mut rows: rusqlite::Rows) -> rusqlite::Result<Self> {
    let columns: Arc<Vec<Column>> = Arc::new(rows.as_ref().map_or(vec![], columns));

    let mut result = vec![];
    while let Some(row) = rows.next()? {
      result.push(Row::from_row(row, Some(columns.clone()))?);
    }

    Ok(Self(result, columns))
  }

  #[cfg(test)]
  pub fn len(&self) -> usize {
    self.0.len()
  }

  pub fn iter(&self) -> std::slice::Iter<'_, Row> {
    self.0.iter()
  }

  pub fn column_count(&self) -> usize {
    self.1.len()
  }

  pub fn column_names(&self) -> Vec<&str> {
    self.1.iter().map(|s| s.name.as_str()).collect()
  }

  pub fn column_name(&self, idx: usize) -> Option<&str> {
    self.1.get(idx).map(|c| c.name.as_str())
  }

  pub fn column_type(&self, idx: usize) -> std::result::Result<ValueType, rusqlite::Error> {
    if let Some(c) = self.1.get(idx) {
      return c.decl_type.ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(
          idx,
          self.column_name(idx).unwrap_or("?").to_string(),
          types::Type::Null,
        )
      });
    }
    return Err(rusqlite::Error::InvalidColumnType(
      idx,
      self.column_name(idx).unwrap_or("?").to_string(),
      types::Type::Null,
    ));
  }
}

#[derive(Debug)]
pub struct Row(Vec<types::Value>, Arc<Vec<Column>>);

impl Row {
  pub fn from_row(row: &rusqlite::Row, cols: Option<Arc<Vec<Column>>>) -> rusqlite::Result<Self> {
    let columns = cols.unwrap_or_else(|| Arc::new(columns(row.as_ref())));

    let count = columns.len();
    let mut values = Vec::<types::Value>::with_capacity(count);
    for idx in 0..count {
      values.push(row.get_ref(idx)?.into());
    }

    Ok(Self(values, columns))
  }

  pub fn get<T>(&self, idx: usize) -> types::FromSqlResult<T>
  where
    T: types::FromSql,
  {
    let val = self
      .0
      .get(idx)
      .ok_or_else(|| types::FromSqlError::Other("Index out of bounds".into()))?;
    T::column_result(val.into())
  }

  pub fn get_value(&self, idx: usize) -> Result<types::Value> {
    self
      .0
      .get(idx)
      .ok_or_else(|| Error::Other("Index out of bounds".into()))
      .cloned()
  }

  pub fn column_count(&self) -> usize {
    self.0.len()
  }

  pub fn column_names(&self) -> Vec<&str> {
    self.1.iter().map(|s| s.name.as_str()).collect()
  }

  pub fn column_name(&self, idx: usize) -> Option<&str> {
    self.1.get(idx).map(|c| c.name.as_str())
  }
}

struct ConnectionState {
  #[allow(unused)]
  f: Box<dyn Fn() -> rusqlite::Connection + Send + Sync>,

  #[cfg(debug_assertions)]
  conn: parking_lot::Mutex<RefCell<rusqlite::Connection>>,
  #[cfg(not(debug_assertions))]
  conn: thread_local::ThreadLocal<RefCell<rusqlite::Connection>>,
}

/// A handle to call functions in background thread.
#[derive(Clone)]
pub struct Connection {
  // NOTE: If we ever wanted to provide a close, we should probaly make this an Arc<Option<>>
  // since one can never consume an Arc.
  state: Arc<ConnectionState>,
}

impl Connection {
  pub fn from_conn(f: impl Fn() -> rusqlite::Connection + Send + Sync + 'static) -> Self {
    #[cfg(debug_assertions)]
    let conn = parking_lot::Mutex::new(RefCell::new(f()));
    #[cfg(not(debug_assertions))]
    let conn = thread_local::ThreadLocal::new();

    return Self {
      state: Arc::new(ConnectionState {
        f: Box::new(f),
        conn,
      }),
    };
  }

  /// Open a new connection to an in-memory SQLite database.
  pub fn open_in_memory() -> Self {
    return Self::from_conn(|| rusqlite::Connection::open_in_memory().unwrap());
  }

  #[inline]
  fn _call<T, F>(&self, f: F) -> Result<T>
  where
    F: FnOnce(&mut rusqlite::Connection) -> Result<T>,
  {
    let state = &self.state;
    #[cfg(debug_assertions)]
    let cell = state.conn.lock();
    #[cfg(not(debug_assertions))]
    let cell = state.conn.get_or(|| RefCell::new((state.f)()));
    return f(&mut cell.borrow_mut());
  }

  #[inline]
  pub fn call<F, R>(&self, function: F) -> Result<R>
  where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R> + 'static + Send,
    R: Send + 'static,
  {
    return self._call(function);
  }

  /// Query SQL statement.
  pub fn query(&self, sql: &str, params: impl Params + Send + 'static) -> Result<Rows> {
    let sql = sql.to_string();
    return self._call(move |conn: &mut rusqlite::Connection| {
      let mut stmt = conn.prepare(&sql)?;
      params.bind(&mut stmt)?;
      let rows = stmt.raw_query();
      Ok(Rows::from_rows(rows)?)
    });
  }

  pub fn query_row(&self, sql: &str, params: impl Params + Send + 'static) -> Result<Option<Row>> {
    let sql = sql.to_string();
    return self._call(move |conn: &mut rusqlite::Connection| {
      let mut stmt = conn.prepare(&sql)?;
      params.bind(&mut stmt)?;
      let mut rows = stmt.raw_query();
      if let Some(row) = rows.next()? {
        return Ok(Some(Row::from_row(row, None)?));
      }
      Ok(None)
    });
  }

  pub fn query_row_map<T, F>(
    &self,
    sql: &str,
    params: impl Params + Send + 'static,
    f: F,
  ) -> Result<Option<T>>
  where
    F: FnOnce(&rusqlite::Row<'_>) -> Result<T>,
  {
    let sql = sql.to_string();
    return self._call(move |conn: &mut rusqlite::Connection| {
      let mut stmt = conn.prepare(&sql)?;
      params.bind(&mut stmt)?;
      let mut rows = stmt.raw_query();
      if let Some(row) = rows.next()? {
        return Ok(Some(f(row)?));
      }
      Ok(None)
    });
  }

  pub fn query_value<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: &str,
    params: impl Params + Send + 'static,
  ) -> Result<Option<T>> {
    let sql = sql.to_string();
    return self._call(move |conn: &mut rusqlite::Connection| {
      let mut stmt = conn.prepare(&sql)?;
      params.bind(&mut stmt)?;
      let mut rows = stmt.raw_query();
      if let Some(row) = rows.next()? {
        return Ok(Some(serde_rusqlite::from_row(row)?));
      }
      Ok(None)
    });
  }

  pub fn query_values<T: serde::de::DeserializeOwned + Send + 'static>(
    &self,
    sql: &str,
    params: impl Params + Send + 'static,
  ) -> Result<Vec<T>> {
    let sql = sql.to_string();
    return self._call(move |conn: &mut rusqlite::Connection| {
      let mut stmt = conn.prepare(&sql)?;
      params.bind(&mut stmt)?;
      let mut rows = stmt.raw_query();

      let mut values = vec![];
      while let Some(row) = rows.next()? {
        values.push(serde_rusqlite::from_row(row)?);
      }
      return Ok(values);
    });
  }

  /// Execute SQL statement.
  pub fn execute(&self, sql: &str, params: impl Params + Send + 'static) -> Result<usize> {
    let sql = sql.to_string();
    return self._call(move |conn: &mut rusqlite::Connection| {
      let mut stmt = conn.prepare(&sql)?;
      params.bind(&mut stmt)?;
      Ok(stmt.raw_execute()?)
    });
  }

  /// Batch execute SQL statements and return rows of last statement.
  pub fn execute_batch(&self, sql: &str) -> Result<Option<Rows>> {
    let sql = sql.to_string();
    return self._call(move |conn: &mut rusqlite::Connection| {
      let batch = rusqlite::Batch::new(conn, &sql);

      let mut p = batch.peekable();
      while let Some(iter) = p.next() {
        let mut stmt = iter?;

        let mut rows = stmt.raw_query();
        let row = rows.next()?;
        if p.peek().is_none() {
          if let Some(row) = row {
            let cols: Arc<Vec<Column>> = Arc::new(columns(row.as_ref()));

            let mut result = vec![Row::from_row(row, Some(cols.clone()))?];
            while let Some(row) = rows.next()? {
              result.push(Row::from_row(row, Some(cols.clone()))?);
            }
            return Ok(Some(Rows(result, cols)));
          }
          return Ok(None);
        }
      }
      return Ok(None);
    });
  }
}

impl Debug for Connection {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Connection").finish()
  }
}
