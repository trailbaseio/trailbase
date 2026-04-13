use crate::Error;
use crate::connection::Connection;
use crate::from_sql::FromSql;
use crate::params::Params;

pub struct DummyConnection;

impl DummyConnection {
  pub async fn read_query_row_get<T>(
    &self,
    _sql: impl AsRef<str> + Send + 'static,
    _params: impl Params + Send + 'static,
    _index: usize,
  ) -> Result<Option<T>, Error>
  where
    T: FromSql + Send + 'static,
  {
    return Err(Error::Other("not implemented".into()));
  }
}

#[allow(unused)]
enum PolymorphicConnection {
  Sqlite(Connection),
  Dummy(DummyConnection),
}

#[allow(unused)]
impl PolymorphicConnection {
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
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn polymorphic_test() {
    let conn = Connection::open_in_memory().unwrap();
    let p = PolymorphicConnection::Sqlite(conn.clone());

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
