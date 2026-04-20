use crate::Error;
use crate::connection::Connection;
use crate::from_sql::FromSql;
use crate::params::Params;
use crate::statement::Statement;
use crate::to_sql::ToSqlProxy;
use crate::value::{Value, ValueRef};

#[derive(Default)]
pub struct DummyClient;

impl DummyClient {
  pub fn query(&self, _sql: &str, _params: Vec<Value>) -> Result<Option<Vec<Value>>, Error> {
    return Ok(Some(vec![Value::Integer(5)]));
  }

  #[allow(unused)]
  pub fn execute(&self, _sql: &str, _params: Vec<Value>) -> Result<u64, Error> {
    return Ok(0);
  }
}

pub struct DummyStatement<'a> {
  #[allow(unused)]
  sql: &'a str,
  params: &'a mut Vec<(usize, Value)>,
}

impl<'a> Statement for DummyStatement<'a> {
  fn bind_parameter(&mut self, one_based_index: usize, param: ToSqlProxy<'_>) -> Result<(), Error> {
    self.params.push((one_based_index, param.try_into()?));
    return Ok(());
  }

  fn parameter_index(&self, _name: &str) -> Result<Option<usize>, Error> {
    return Err(Error::Other("not implemented: parse `self.sql`".into()));
  }
}

#[derive(Default)]
pub struct DummyConnection {
  client: DummyClient,
}

impl DummyConnection {
  pub async fn read_query_row_get<T>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    index: usize,
  ) -> Result<Option<T>, Error>
  where
    T: FromSql + Send + 'static,
  {
    let mut bound: Vec<(usize, Value)> = Vec::new();
    let mut statement = DummyStatement {
      sql: sql.as_ref(),
      params: &mut bound,
    };

    params.bind(&mut statement)?;

    bound.sort_by(|a, b| {
      return a.0.cmp(&b.0);
    });

    let Some(mut row) = self
      .client
      .query(sql.as_ref(), bound.into_iter().map(|p| p.1).collect())?
    else {
      return Ok(None);
    };

    let value: Value = row.remove(index);
    let value_ref: ValueRef = (&value).into();

    return Ok(Some(T::column_result(value_ref)?));
  }
}

#[allow(unused)]
pub enum DummyPolymorphicConnection {
  Sqlite(Connection),
  Dummy(DummyConnection),
}

#[allow(unused)]
impl DummyPolymorphicConnection {
  pub async fn read_query_row_get<T>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    index: usize,
  ) -> Result<Option<T>, Error>
  where
    T: FromSql + Send + 'static,
  {
    match self {
      Self::Sqlite(c) => c.read_query_row_get(sql, params, index).await,
      Self::Dummy(c) => c.read_query_row_get(sql, params, index).await,
    }
  }

  pub fn sqlite_connection(&self) -> Option<&Connection> {
    match self {
      Self::Sqlite(conn) => Some(conn),
      Self::Dummy(_) => None,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn polymorphic_dummy_test() {
    let p = DummyPolymorphicConnection::Dummy(DummyConnection::default());

    let value: i64 = p
      .read_query_row_get("ANYTHING", (), 0)
      .await
      .unwrap()
      .unwrap();

    assert_eq!(5, value);
  }

  #[tokio::test]
  async fn polymorphic_sqlite_test() {
    let conn = Connection::open_in_memory().unwrap();
    let p = DummyPolymorphicConnection::Sqlite(conn.clone());

    conn
      .execute_batch(
        "
          CREATE TABLE test (id INTEGER PRIMARY KEY, value INTEGER) STRICT;
          INSERT INTO test (value) VALUES (4);
        ",
      )
      .await
      .unwrap();

    let value: i64 = p
      .read_query_row_get("SELECT value FROM test ORDER BY id", (), 0)
      .await
      .unwrap()
      .unwrap();

    assert_eq!(4, value);
  }
}
